//! Provides the Reset Chance Component and its settings.

use super::key_value;
use crate::{
    TimerPhase,
    localization::{Lang, Text},
    platform::prelude::*,
    settings::{Color, Field, Gradient, SettingsDescription, Value},
    timing::Snapshot,
};
use alloc::borrow::Cow;
use core::fmt::Write as _;
use serde_derive::{Deserialize, Serialize};

/// The value shown by the component.
#[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChanceMode {
    /// The percentage of attempts that ended at the current segment.
    ResetChance,
    /// The percentage of attempts that completed the current segment.
    SuccessChance,
    /// The number of attempts that ended at the current segment.
    RunsEnded,
}

/// The history window used by the calculation.
#[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalculationBasis {
    /// All recorded attempts.
    AllRuns,
    /// A recent window of overall attempts.
    RecentRuns,
    /// A separate recent attempt window for every segment.
    RecentSplitAttempts,
}

/// The number of fractional digits shown for percentages.
#[derive(Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PercentageAccuracy {
    /// No fractional digits.
    Integer,
    /// At most one fractional digit.
    Tenths,
    /// At most two fractional digits.
    Hundredths,
}

/// The settings for the Reset Chance Component.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The background shown behind the component.
    pub background: Gradient,
    /// The value shown by the component.
    pub chance_mode: ChanceMode,
    /// The accuracy of percentages.
    pub accuracy: PercentageAccuracy,
    /// Whether insignificant fractional zeroes are shown.
    pub show_trailing_zeroes: bool,
    /// The history window used by the calculation.
    pub calculation_basis: CalculationBasis,
    /// The size of the recent overall-attempt window.
    pub recent_runs: u64,
    /// The size of each recent segment-attempt window.
    pub recent_split_attempts: u64,
    /// Whether the label and value are displayed on separate rows.
    pub display_two_rows: bool,
    /// The label color, or the layout's color if absent.
    pub label_color: Option<Color>,
    /// The value color, or the layout's color if absent.
    pub value_color: Option<Color>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            background: key_value::DEFAULT_GRADIENT,
            chance_mode: ChanceMode::ResetChance,
            accuracy: PercentageAccuracy::Integer,
            show_trailing_zeroes: false,
            calculation_basis: CalculationBasis::AllRuns,
            recent_runs: 100,
            recent_split_attempts: 50,
            display_two_rows: false,
            label_color: None,
            value_color: None,
        }
    }
}

/// Shows the historical likelihood of an attempt ending at the current split.
#[derive(Default, Clone)]
pub struct Component {
    settings: Settings,
}

impl Component {
    /// Creates a new Reset Chance Component.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a Reset Chance Component with the given settings.
    pub const fn with_settings(settings: Settings) -> Self {
        Self { settings }
    }

    /// Accesses the component's settings.
    pub const fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Grants mutable access to the component's settings.
    pub const fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    /// Accesses the component's localized name.
    pub const fn name(&self, lang: Lang) -> Cow<'static, str> {
        Cow::Borrowed(self.label(lang))
    }

    const fn label(&self, lang: Lang) -> &'static str {
        match self.settings.chance_mode {
            ChanceMode::ResetChance => Text::ComponentResetChance.resolve(lang),
            ChanceMode::SuccessChance => Text::ComponentSuccessChance.resolve(lang),
            ChanceMode::RunsEnded => Text::ComponentRunsEnded.resolve(lang),
        }
    }

    fn count_at_or_after(timer: &Snapshot<'_>, segment: usize, minimum: i32) -> usize {
        timer.run().segments()[segment]
            .segment_history()
            .iter_actual_runs()
            .filter(|(index, _)| *index >= minimum)
            .count()
    }

    fn all_runs(&self, timer: &Snapshot<'_>, segment: usize) -> Option<(usize, usize)> {
        let completions = Self::count_at_or_after(timer, segment, 1);
        let attempts = if segment == 0 {
            timer.run().attempt_history().len()
        } else {
            Self::count_at_or_after(timer, segment - 1, 1)
        };
        valid_counts(completions, attempts)
    }

    fn recent_runs(&self, timer: &Snapshot<'_>, segment: usize) -> Option<(usize, usize)> {
        let window = usize::try_from(self.settings.recent_runs).unwrap_or(usize::MAX);
        if window == 0 {
            return None;
        }
        let attempts_history = timer.run().attempt_history();
        let latest = attempts_history
            .iter()
            .map(|attempt| attempt.index())
            .max()?;
        let window_i32 = i32::try_from(window).unwrap_or(i32::MAX);
        let minimum = latest.saturating_add(1).saturating_sub(window_i32).max(1);
        let completions = Self::count_at_or_after(timer, segment, minimum);
        let attempts = if segment == 0 {
            attempts_history.len().min(window)
        } else {
            Self::count_at_or_after(timer, segment - 1, minimum)
        };
        valid_counts(completions, attempts)
    }

    fn recent_split_attempts(
        &self,
        timer: &Snapshot<'_>,
        segment: usize,
    ) -> Option<(usize, usize)> {
        let window = usize::try_from(self.settings.recent_split_attempts).unwrap_or(usize::MAX);
        if window == 0 {
            return None;
        }
        if segment == 0 {
            let attempts_history = timer.run().attempt_history();
            let latest = attempts_history
                .iter()
                .map(|attempt| attempt.index())
                .max()?;
            let window_i32 = i32::try_from(window).unwrap_or(i32::MAX);
            let minimum = latest.saturating_add(1).saturating_sub(window_i32).max(1);
            let completions = Self::count_at_or_after(timer, 0, minimum);
            return valid_counts(completions, attempts_history.len().min(window));
        }

        let mut previous = timer.run().segments()[segment - 1]
            .segment_history()
            .iter_actual_runs();
        let previous_count = previous.clone().count();
        let attempts = previous_count.min(window);
        if attempts == 0 {
            return None;
        }
        let minimum = if previous_count <= window {
            1
        } else {
            previous
                .nth(previous_count - window)
                .map(|(index, _)| *index)?
        };
        let completions = Self::count_at_or_after(timer, segment, minimum);
        valid_counts(completions, attempts)
    }

    fn counts(&self, timer: &Snapshot<'_>, segment: usize) -> Option<(usize, usize)> {
        if segment >= timer.run().len() {
            return None;
        }
        match self.settings.calculation_basis {
            CalculationBasis::AllRuns => self.all_runs(timer, segment),
            CalculationBasis::RecentRuns => self.recent_runs(timer, segment),
            CalculationBasis::RecentSplitAttempts => self.recent_split_attempts(timer, segment),
        }
    }

    fn value(&self, timer: &Snapshot<'_>) -> Option<f64> {
        if matches!(
            timer.current_phase(),
            TimerPhase::NotRunning | TimerPhase::Ended
        ) {
            return Some(0.0);
        }
        let segment = timer.current_split_index()?;
        let (completions, attempts) = self.counts(timer, segment)?;
        let completions = completions as f64;
        let attempts = attempts as f64;
        Some(match self.settings.chance_mode {
            ChanceMode::ResetChance => (1.0 - completions / attempts) * 100.0,
            ChanceMode::SuccessChance => completions / attempts * 100.0,
            ChanceMode::RunsEnded => attempts - completions,
        })
    }

    fn format_value(&self, output: &mut String, value: Option<f64>, lang: Lang) {
        let Some(value) = value else {
            output.push('?');
            return;
        };
        if self.settings.chance_mode == ChanceMode::RunsEnded {
            let _ = write!(output, "{value:.0}");
            return;
        }
        let digits = match self.settings.accuracy {
            PercentageAccuracy::Integer => 0,
            PercentageAccuracy::Tenths => 1,
            PercentageAccuracy::Hundredths => 2,
        };
        let mut formatted = format!("{value:.digits$}");
        if !self.settings.show_trailing_zeroes && digits != 0 {
            while formatted.ends_with('0') {
                formatted.pop();
            }
            if formatted.ends_with('.') {
                formatted.pop();
            }
        }
        if lang.decimal_separator().get() != b'.' {
            formatted = formatted.replace('.', lang.decimal_separator().as_str());
        }
        output.push_str(&formatted);
        output.push('%');
    }

    /// Updates the component's state.
    pub fn update_state(&self, state: &mut key_value::State, timer: &Snapshot<'_>, lang: Lang) {
        state.background = self.settings.background;
        state.key_color = self.settings.label_color;
        state.value_color = self.settings.value_color;
        state.semantic_color = Default::default();
        state.key.clear();
        state.key.push_str(self.label(lang));
        state.value.clear();
        self.format_value(&mut state.value, self.value(timer), lang);
        state.key_abbreviations.clear();
        state.key_abbreviations.push(
            match self.settings.chance_mode {
                ChanceMode::ResetChance => Text::ResetChanceShort,
                ChanceMode::SuccessChance => Text::SuccessChanceShort,
                ChanceMode::RunsEnded => Text::RunsEndedShort,
            }
            .resolve(lang)
            .into(),
        );
        state.display_two_rows = self.settings.display_two_rows;
        state.updates_frequently = false;
    }

    /// Calculates the component's state.
    pub fn state(&self, timer: &Snapshot<'_>, lang: Lang) -> key_value::State {
        let mut state = key_value::State::default();
        self.update_state(&mut state, timer, lang);
        state
    }

    /// Describes the component's settings.
    pub fn settings_description(&self, lang: Lang) -> SettingsDescription {
        let fields = [
            (
                Text::ResetChanceBackground,
                Text::ResetChanceBackgroundDescription,
                self.settings.background.into(),
            ),
            (
                Text::ResetChanceMode,
                Text::ResetChanceModeDescription,
                self.settings.chance_mode.into(),
            ),
            (
                Text::ResetChanceAccuracy,
                Text::ResetChanceAccuracyDescription,
                self.settings.accuracy.into(),
            ),
            (
                Text::ResetChanceTrailingZeroes,
                Text::ResetChanceTrailingZeroesDescription,
                self.settings.show_trailing_zeroes.into(),
            ),
            (
                Text::ResetChanceBasis,
                Text::ResetChanceBasisDescription,
                self.settings.calculation_basis.into(),
            ),
            (
                Text::ResetChanceRecentRuns,
                Text::ResetChanceRecentRunsDescription,
                self.settings.recent_runs.into(),
            ),
            (
                Text::ResetChanceRecentSplitAttempts,
                Text::ResetChanceRecentSplitAttemptsDescription,
                self.settings.recent_split_attempts.into(),
            ),
            (
                Text::ResetChanceDisplayTwoRows,
                Text::ResetChanceDisplayTwoRowsDescription,
                self.settings.display_two_rows.into(),
            ),
            (
                Text::ResetChanceLabelColor,
                Text::ResetChanceLabelColorDescription,
                self.settings.label_color.into(),
            ),
            (
                Text::ResetChanceValueColor,
                Text::ResetChanceValueColorDescription,
                self.settings.value_color.into(),
            ),
        ];
        SettingsDescription::with_fields(
            fields
                .into_iter()
                .map(|(name, description, value)| {
                    Field::new(
                        name.resolve(lang).into(),
                        description.resolve(lang).into(),
                        value,
                    )
                })
                .collect(),
        )
    }

    /// Sets a setting by its stable index.
    pub fn set_value(&mut self, index: usize, value: Value) {
        match index {
            0 => self.settings.background = value.into(),
            1 => self.settings.chance_mode = value.into(),
            2 => self.settings.accuracy = value.into(),
            3 => self.settings.show_trailing_zeroes = value.into(),
            4 => self.settings.calculation_basis = value.into(),
            5 => self.settings.recent_runs = value.into(),
            6 => self.settings.recent_split_attempts = value.into(),
            7 => self.settings.display_two_rows = value.into(),
            8 => self.settings.label_color = value.into(),
            9 => self.settings.value_color = value.into(),
            _ => panic!("Unsupported Setting Index"),
        }
    }
}

fn valid_counts(completions: usize, attempts: usize) -> Option<(usize, usize)> {
    (attempts != 0 && completions != 0 && completions <= attempts)
        .then_some((completions, attempts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Run, Segment, Time, TimeSpan, Timer};

    fn timer(attempt_ids: &[i32], histories: &[&[i32]], running: bool) -> Timer {
        let mut run = Run::new();
        let history_time = Time::new().with_real_time(Some(TimeSpan::from_seconds(1.0)));
        for (segment_index, ids) in histories.iter().enumerate() {
            let mut segment = Segment::new(format!("Segment {segment_index}"));
            segment.set_best_segment_time(history_time);
            for &id in *ids {
                segment.segment_history_mut().insert(id, history_time);
            }
            run.push_segment(segment);
        }
        for &id in attempt_ids {
            run.add_attempt_with_index(Time::new(), id, None, None, None);
        }
        let mut timer = Timer::new(run).unwrap();
        if running {
            timer.start().unwrap();
        }
        timer
    }

    #[test]
    fn all_modes_and_artifacts() {
        let timer = timer(&[1, 2, 3, 4], &[&[-1, 0, 1, 2, 3]], true);
        let mut component = Component::new();
        assert_eq!(
            component.state(&timer.snapshot(), Lang::English).value,
            "25%"
        );
        component.settings.chance_mode = ChanceMode::SuccessChance;
        assert_eq!(
            component.state(&timer.snapshot(), Lang::English).value,
            "75%"
        );
        component.settings.chance_mode = ChanceMode::RunsEnded;
        assert_eq!(component.state(&timer.snapshot(), Lang::English).value, "1");
    }

    #[test]
    fn later_segment_uses_previous_completions() {
        let timer = timer(&[1, 2, 3, 4], &[&[1, 2, 3], &[1, 3]], true);
        let mut timer = timer;
        timer.split().unwrap();
        assert_eq!(
            Component::new()
                .state(&timer.snapshot(), Lang::English)
                .value,
            "33%"
        );
    }

    #[test]
    fn recent_run_and_split_windows() {
        let timer = timer(
            &[1, 4, 8, 10],
            &[&[1, 4, 8, 10], &[1, 8, 10], &[1, 10]],
            true,
        );
        let mut component = Component::new();
        component.settings.calculation_basis = CalculationBasis::RecentRuns;
        component.settings.recent_runs = 3;
        assert_eq!(component.counts(&timer.snapshot(), 1), Some((2, 2)));
        component.settings.calculation_basis = CalculationBasis::RecentSplitAttempts;
        component.settings.recent_split_attempts = 2;
        assert_eq!(component.counts(&timer.snapshot(), 2), Some((1, 2)));
    }

    #[test]
    fn unavailable_and_inconsistent_histories_are_safe() {
        let empty = timer(&[], &[&[]], true);
        assert_eq!(
            Component::new()
                .state(&empty.snapshot(), Lang::English)
                .value,
            "?"
        );
        assert_eq!(valid_counts(2, 1), None);
        let inconsistent = timer(&[1], &[&[1, 2]], true);
        let mut component = Component::new();
        component.settings.calculation_basis = CalculationBasis::RecentRuns;
        component.settings.recent_runs = 0;
        assert_eq!(
            component
                .state(&inconsistent.snapshot(), Lang::English)
                .value,
            "?"
        );
    }

    #[test]
    fn inactive_timer_shows_zero() {
        let timer = timer(&[], &[&[]], false);
        assert_eq!(
            Component::new()
                .state(&timer.snapshot(), Lang::English)
                .value,
            "0%"
        );
    }

    #[test]
    fn percentage_formatting_and_state_settings() {
        let timer = timer(&[1, 2, 3], &[&[1, 2]], true);
        let mut component = Component::new();
        component.settings.accuracy = PercentageAccuracy::Hundredths;
        assert_eq!(
            component.state(&timer.snapshot(), Lang::English).value,
            "33.33%"
        );
        component.settings.show_trailing_zeroes = true;
        component.settings.accuracy = PercentageAccuracy::Tenths;
        component.settings.display_two_rows = true;
        component.settings.label_color = Some(Color::rgba8(1, 2, 3, 255));
        let state = component.state(&timer.snapshot(), Lang::German);
        assert_eq!(state.value, "33,3%");
        assert!(state.display_two_rows);
        assert_eq!(state.key_color, component.settings.label_color);
        assert!(!state.updates_frequently);
    }

    #[test]
    fn settings_have_stable_shape() {
        let mut component = Component::new();
        assert_eq!(
            component.settings_description(Lang::English).fields.len(),
            10
        );
        component.set_value(1, ChanceMode::RunsEnded.into());
        component.set_value(2, PercentageAccuracy::Tenths.into());
        component.set_value(4, CalculationBasis::RecentRuns.into());
        component.set_value(5, 12_u64.into());
        assert!(component.settings.chance_mode == ChanceMode::RunsEnded);
        assert!(component.settings.accuracy == PercentageAccuracy::Tenths);
        assert!(component.settings.calculation_basis == CalculationBasis::RecentRuns);
        assert_eq!(component.settings.recent_runs, 12);
    }
}
