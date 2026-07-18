//! Provides the Alternate Timing Method component which displays the current time in the opposite timing method to the one currently used.
//! Example: when comparing against Game Time, this component will display the time in Real Time

use super::key_value;
use crate::{
    Timer, TimingMethod,
    localization::{Lang, Text},
    platform::prelude::*,
    settings::{Color, Field, Gradient, SettingsDescription, Value},
    timing::formatter::{Accuracy, DigitsFormat, TimeFormatter, timer as formatter},
};
use core::fmt::Write;
use serde_derive::{Deserialize, Serialize};

/// The Alternative Timing Component is a component that shows the time of the
/// timing method that's the opposite of the main one.
#[derive(Default, Clone)]
pub struct Component {
    settings: Settings,
}

/// The Settings for this component.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The background shown behind the component.
    pub background: Gradient,
    /// Specifies whether to display the name of the component and its value in
    /// two separate rows.
    pub display_two_rows: bool,
    /// The color of the label. If [`None`] is specified, the color is taken from
    /// the layout.
    pub label_color: Option<Color>,
    /// The color of the value. If [`None`] is specified, the color is taken from
    /// the layout.
    pub value_color: Option<Color>,
    /// Determines how many digits are to always be shown. If the duration is
    /// lower than the digits to be shown, they are filled up with zeros.
    pub digits_format: DigitsFormat,
    /// The accuracy of the time shown.
    pub accuracy: Accuracy,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            background: key_value::DEFAULT_GRADIENT,
            display_two_rows: false,
            label_color: None,
            value_color: None,
            digits_format: DigitsFormat::SingleDigitSeconds,
            accuracy: Accuracy::Hundredths,
        }
    }
}

impl Component {
    /// Creates a new Alternate Timing Method component
    pub fn new() -> Self {
        Default::default()
    }

    /// Creates a new Alternate Timing Method with the given settings.
    pub const fn with_settings(settings: Settings) -> Self {
        Self { settings }
    }

    /// Accesses the settings of the component.
    pub const fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Grants mutable access to the settings of the component.
    pub const fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    /// Accesses the name of the component for the specified language.
    pub const fn name(&self) -> &'static str {
        "Alternate Timing Method"
    }

    /// Updates the component's state based on the timer provided.
    pub fn update_state(&self, state: &mut key_value::State, timer: &Timer, lang: Lang) {
        let timing_method_to_show = match timer.current_timing_method() {
            TimingMethod::RealTime => TimingMethod::GameTime,
            TimingMethod::GameTime => TimingMethod::RealTime,
        };

        let current_time = timer.snapshot().current_time();
        let time_to_show = current_time[timing_method_to_show].or(current_time.real_time);

        state.background = self.settings.background;
        state.key_color = self.settings.label_color;
        state.value_color = self.settings.value_color;
        state.semantic_color = Default::default();

        state.key.clear();
        state.key.push_str(match timing_method_to_show {
            TimingMethod::RealTime => Text::RealTime.resolve(lang),
            TimingMethod::GameTime => Text::GameTime.resolve(lang),
        });

        state.value.clear();
        let formatted_time = formatter::Time::with_digits_format(self.settings.digits_format)
            .format(time_to_show, lang);
        let formatted_fractions =
            formatter::Fraction::with_accuracy(self.settings.accuracy).format(time_to_show, lang);
        let _ = write!(state.value, "{}{}", formatted_time, formatted_fractions);

        state.key_abbreviations.clear();
        state.display_two_rows = self.settings.display_two_rows;
        state.updates_frequently = timer
            .current_phase()
            .updates_frequently(timing_method_to_show)
            && time_to_show.is_some();
    }

    /// Calculates the component's state based on the timer provided.
    pub fn state(&self, timer: &Timer, lang: Lang) -> key_value::State {
        let mut state = Default::default();
        self.update_state(&mut state, timer, lang);
        state
    }

    /// Accesses a generic description of the settings available for this
    /// component and their current values for the specified language.
    pub fn settings_description(&self, lang: Lang) -> SettingsDescription {
        SettingsDescription::with_fields(vec![
            Field::new(
                Text::CurrentComparisonBackground.resolve(lang).into(),
                Text::CurrentComparisonBackgroundDescription
                    .resolve(lang)
                    .into(),
                self.settings.background.into(),
            ),
            Field::new(
                Text::CurrentComparisonDisplayTwoRows.resolve(lang).into(),
                Text::CurrentComparisonDisplayTwoRowsDescription
                    .resolve(lang)
                    .into(),
                self.settings.display_two_rows.into(),
            ),
            Field::new(
                Text::CurrentComparisonLabelColor.resolve(lang).into(),
                Text::CurrentComparisonLabelColorDescription
                    .resolve(lang)
                    .into(),
                self.settings.label_color.into(),
            ),
            Field::new(
                Text::CurrentComparisonValueColor.resolve(lang).into(),
                Text::CurrentComparisonValueColorDescription
                    .resolve(lang)
                    .into(),
                self.settings.value_color.into(),
            ),
            Field::new(
                Text::DigitsFormat.resolve(lang).into(),
                Text::DigitsFormatDescription.resolve(lang).into(),
                self.settings.digits_format.into(),
            ),
            Field::new(
                Text::Accuracy.resolve(lang).into(),
                Text::AccuracyDescription.resolve(lang).into(),
                self.settings.accuracy.into(),
            ),
        ])
    }

    /// Sets a setting's value by its index to the given value.
    ///
    /// # Panics
    ///
    /// This panics if the type of the value to be set is not compatible with
    /// the type of the setting's value. A panic can also occur if the index of
    /// the setting provided is out of bounds.
    pub fn set_value(&mut self, index: usize, value: Value) {
        match index {
            0 => self.settings.background = value.into(),
            1 => self.settings.display_two_rows = value.into(),
            2 => self.settings.label_color = value.into(),
            3 => self.settings.value_color = value.into(),
            4 => self.settings.digits_format = value.into(),
            5 => self.settings.accuracy = value.into(),
            _ => panic!("Unsupported Setting Index"),
        }
    }
}
