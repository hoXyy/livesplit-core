//! Provides a component for showing the world record from speedrun.com.
//!
//! This component follows the behavior of LiveSplit's C# World Record
//! component. It derives the game, category, and optional leaderboard filters
//! from the current run rather than duplicating them in component settings.
//!
//! HTTP is transport-neutral: [`Component::request_url`] returns the next URL
//! to fetch and [`Component::parse_response`] consumes the response body.

use super::key_value;
use crate::{
    Lang, TimerPhase, TimingMethod,
    platform::prelude::*,
    settings::{Color, Field, Gradient, SettingsDescription, Value},
    timing::{
        Snapshot, TimeSpan, TimeStamp,
        formatter::{Accuracy, Regular, TimeFormatter},
    },
};
use core::fmt::Write;
use serde_derive::{Deserialize, Serialize};

const API_BASE: &str = "https://www.speedrun.com/api/v1";
const REFRESH_SECONDS: f64 = 5.0 * 60.0;

/// The World Record Component shows the first-place run for the current game
/// and category on speedrun.com.
#[derive(Clone)]
pub struct Component {
    settings: Settings,
    lookup: Lookup,
}

/// The settings for the World Record Component.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// The background shown behind the component.
    pub background: Gradient,
    /// Whether the label and value are displayed in two separate rows.
    pub display_two_rows: bool,
    /// Filters the leaderboard to the run's non-subcategory variables.
    pub filter_variables: bool,
    /// Filters the leaderboard to the run's subcategory variables.
    pub filter_subcategories: bool,
    /// Filters the leaderboard to the run's platform and emulator usage.
    pub filter_platform: bool,
    /// Filters the leaderboard to the run's region.
    pub filter_region: bool,
    /// Overrides the leaderboard timing method. `None` uses the leaderboard's
    /// primary timing method.
    pub timing_method: Option<TimingMethodOverride>,
    /// Automatically uses millisecond precision only if the leaderboard time
    /// has a fractional part.
    pub automatic_precision: bool,
    /// The precision used when automatic precision is disabled.
    pub accuracy: Accuracy,
    /// The color of the label. If `None`, the layout's color is used.
    pub label_color: Option<Color>,
    /// The color of the value. If `None`, the layout's color is used.
    pub value_color: Option<Color>,
}

/// A speedrun.com timing method that can override the leaderboard default.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimingMethodOverride {
    /// Real time including loads.
    RealTime,
    /// Real time with loads removed.
    RealTimeWithoutLoads,
    /// The game's internal timing method.
    GameTime,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            background: key_value::DEFAULT_GRADIENT,
            display_two_rows: false,
            filter_variables: false,
            filter_subcategories: true,
            filter_platform: false,
            filter_region: false,
            timing_method: None,
            automatic_precision: true,
            accuracy: Accuracy::Milliseconds,
            label_color: None,
            value_color: None,
        }
    }
}

#[derive(Clone, Default)]
struct Lookup {
    query: Option<Query>,
    stage: Stage,
    game_id: String,
    category_id: String,
    platform_id: String,
    region_id: String,
    variable_filters: Vec<(String, String)>,
    records: Vec<Record>,
    error: Option<String>,
    completed_at: Option<TimeStamp>,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct Query {
    game: String,
    category: String,
    platform: String,
    region: String,
    uses_emulator: bool,
    variables: Vec<(String, String)>,
    filter_variables: bool,
    filter_subcategories: bool,
    filter_platform: bool,
    filter_region: bool,
    timing_method: Option<TimingMethodOverride>,
}

#[derive(Clone, Copy, Default, PartialEq, Eq)]
enum Stage {
    #[default]
    Game,
    Category,
    Variables,
    Platforms,
    Regions,
    Leaderboard,
    Complete,
}

#[derive(Clone, Default)]
struct Times {
    primary: Option<f64>,
    real_time: Option<f64>,
    real_time_without_loads: Option<f64>,
    game_time: Option<f64>,
}

#[derive(Clone)]
struct Record {
    times: Times,
    runners: Vec<String>,
}

impl Record {
    fn time(&self, method: Option<TimingMethodOverride>) -> Option<f64> {
        match method {
            None => self.times.primary,
            Some(TimingMethodOverride::RealTime) => self.times.real_time,
            Some(TimingMethodOverride::RealTimeWithoutLoads) => self.times.real_time_without_loads,
            Some(TimingMethodOverride::GameTime) => self.times.game_time,
        }
    }
}

/// An error encountered while consuming a speedrun.com API response.
#[derive(Debug)]
pub enum ParseResponseError {
    /// The response is not valid JSON.
    Json {
        /// A description of the JSON parsing error.
        message: String,
    },
    /// The response does not contain the expected data.
    InvalidResponse {
        /// A description of the invalid or missing data.
        message: String,
    },
}

impl core::fmt::Display for ParseResponseError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Json { message } => write!(
                formatter,
                "The speedrun.com response is not valid JSON: {message}"
            ),
            Self::InvalidResponse { message } => formatter.write_str(message),
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for ParseResponseError {}

impl Default for Component {
    fn default() -> Self {
        Self::with_settings(Settings::default())
    }
}

impl Component {
    /// Creates a new World Record Component.
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new World Record Component with the given settings.
    pub fn with_settings(settings: Settings) -> Self {
        Self {
            settings,
            lookup: Lookup::default(),
        }
    }

    /// Accesses the settings of the component.
    pub const fn settings(&self) -> &Settings {
        &self.settings
    }

    /// Grants mutable access to the settings of the component.
    pub const fn settings_mut(&mut self) -> &mut Settings {
        &mut self.settings
    }

    /// Accesses the name of the component.
    pub const fn name(&self) -> &'static str {
        "World Record"
    }

    /// Restarts the speedrun.com lookup.
    pub fn refresh(&mut self) {
        self.lookup = Lookup::default();
    }

    /// Returns the URL of the next speedrun.com API request.
    ///
    /// The lookup automatically follows the current run's game, category, and
    /// metadata, and refreshes a completed result every five minutes.
    pub fn request_url(&mut self, timer: &Snapshot<'_>) -> Option<String> {
        self.sync_query(timer);
        if self.lookup.error.is_some() {
            return None;
        }
        let query = self.lookup.query.as_ref()?;
        if query.game.is_empty() || query.category.is_empty() {
            return None;
        }

        Some(match self.lookup.stage {
            Stage::Game => format!("{API_BASE}/games?name={}", percent_encode(&query.game)),
            Stage::Category => format!(
                "{API_BASE}/games/{}/categories",
                percent_encode(&self.lookup.game_id)
            ),
            Stage::Variables => format!(
                "{API_BASE}/categories/{}/variables",
                percent_encode(&self.lookup.category_id)
            ),
            Stage::Platforms => format!("{API_BASE}/platforms?max=200"),
            Stage::Regions => format!("{API_BASE}/regions?max=200"),
            Stage::Leaderboard => self.leaderboard_url(),
            Stage::Complete => return None,
        })
    }

    /// Consumes the response body for the URL returned by
    /// [`request_url`](Self::request_url).
    pub fn parse_response(&mut self, response: &str) -> Result<(), ParseResponseError> {
        let value: serde_json::Value =
            serde_json::from_str(response).map_err(|error| ParseResponseError::Json {
                message: error.to_string(),
            })?;

        let result = match self.lookup.stage {
            Stage::Game => self.parse_game(&value),
            Stage::Category => self.parse_category(&value),
            Stage::Variables => self.parse_variables(&value),
            Stage::Platforms => self.parse_platforms(&value),
            Stage::Regions => self.parse_regions(&value),
            Stage::Leaderboard => self.parse_leaderboard(&value),
            Stage::Complete => Ok(()),
        };
        if let Err(error) = &result {
            self.lookup.error = Some(error.to_string());
        }
        result
    }

    fn parse_game(&mut self, value: &serde_json::Value) -> Result<(), ParseResponseError> {
        let query = self.lookup.query.as_ref().unwrap();
        let games = data_array(value)?;
        let game = find_named(games, &query.game, "/names/international")
            .or_else(|| games.first())
            .ok_or_else(|| invalid(format!("Game not found: {}", query.game)))?;
        self.lookup.game_id = required_str(game, "/id")?.into();
        self.lookup.stage = Stage::Category;
        Ok(())
    }

    fn parse_category(&mut self, value: &serde_json::Value) -> Result<(), ParseResponseError> {
        let query = self.lookup.query.as_ref().unwrap();
        let categories = data_array(value)?;
        let category = find_named(categories, &query.category, "/name")
            .ok_or_else(|| invalid(format!("Category not found: {}", query.category)))?;
        self.lookup.category_id = required_str(category, "/id")?.into();
        self.advance_after_category();
        Ok(())
    }

    fn parse_variables(&mut self, value: &serde_json::Value) -> Result<(), ParseResponseError> {
        let query = self.lookup.query.as_ref().unwrap();
        for variable in data_array(value)? {
            let is_subcategory = variable
                .pointer("/is-subcategory")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            if (is_subcategory && !query.filter_subcategories)
                || (!is_subcategory && !query.filter_variables)
            {
                continue;
            }
            let Some((_, selected_value)) = query.variables.iter().find(|(name, _)| {
                variable
                    .pointer("/name")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|variable_name| variable_name.eq_ignore_ascii_case(name))
            }) else {
                continue;
            };
            let Some(choices) = variable
                .pointer("/values/choices")
                .and_then(serde_json::Value::as_object)
            else {
                continue;
            };
            if let Some((choice_id, _)) = choices.iter().find(|(_, choice)| {
                choice
                    .pointer("/label")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|label| label.eq_ignore_ascii_case(selected_value))
            }) {
                self.lookup
                    .variable_filters
                    .push((required_str(variable, "/id")?.into(), choice_id.clone()));
            }
        }
        self.advance_after_variables();
        Ok(())
    }

    fn parse_platforms(&mut self, value: &serde_json::Value) -> Result<(), ParseResponseError> {
        let query = self.lookup.query.as_ref().unwrap();
        if let Some(platform) = find_named(data_array(value)?, &query.platform, "/name") {
            self.lookup.platform_id = required_str(platform, "/id")?.into();
        }
        self.advance_after_platforms();
        Ok(())
    }

    fn parse_regions(&mut self, value: &serde_json::Value) -> Result<(), ParseResponseError> {
        let query = self.lookup.query.as_ref().unwrap();
        if let Some(region) = find_named(data_array(value)?, &query.region, "/name") {
            self.lookup.region_id = required_str(region, "/id")?.into();
        }
        self.lookup.stage = Stage::Leaderboard;
        Ok(())
    }

    fn parse_leaderboard(&mut self, value: &serde_json::Value) -> Result<(), ParseResponseError> {
        let runs = value
            .pointer("/data/runs")
            .and_then(serde_json::Value::as_array)
            .ok_or_else(|| invalid("The response has no leaderboard runs."))?;
        let players = value
            .pointer("/data/players/data")
            .and_then(serde_json::Value::as_array);

        self.lookup.records.clear();
        for entry in runs {
            let run = entry
                .pointer("/run")
                .ok_or_else(|| invalid("A leaderboard entry has no run."))?;
            let mut runners = Vec::new();
            if let Some(run_players) = run
                .pointer("/players")
                .and_then(serde_json::Value::as_array)
            {
                for player in run_players {
                    runners.push(resolve_player(player, players)?);
                }
            }
            self.lookup.records.push(Record {
                times: Times {
                    primary: optional_f64(run, "/times/primary_t"),
                    real_time: optional_f64(run, "/times/realtime_t"),
                    real_time_without_loads: optional_f64(run, "/times/realtime_noloads_t"),
                    game_time: optional_f64(run, "/times/ingame_t"),
                },
                runners,
            });
        }
        self.lookup.stage = Stage::Complete;
        self.lookup.completed_at = Some(TimeStamp::now());
        Ok(())
    }

    fn advance_after_category(&mut self) {
        let query = self.lookup.query.as_ref().unwrap();
        self.lookup.stage = if (query.filter_variables || query.filter_subcategories)
            && !query.variables.is_empty()
        {
            Stage::Variables
        } else if query.filter_platform && !query.platform.is_empty() {
            Stage::Platforms
        } else if query.filter_region && !query.region.is_empty() {
            Stage::Regions
        } else {
            Stage::Leaderboard
        };
    }

    fn advance_after_variables(&mut self) {
        let query = self.lookup.query.as_ref().unwrap();
        self.lookup.stage = if query.filter_platform && !query.platform.is_empty() {
            Stage::Platforms
        } else if query.filter_region && !query.region.is_empty() {
            Stage::Regions
        } else {
            Stage::Leaderboard
        };
    }

    fn advance_after_platforms(&mut self) {
        let query = self.lookup.query.as_ref().unwrap();
        self.lookup.stage = if query.filter_region && !query.region.is_empty() {
            Stage::Regions
        } else {
            Stage::Leaderboard
        };
    }

    fn leaderboard_url(&self) -> String {
        let query = self.lookup.query.as_ref().unwrap();
        let mut url = format!(
            "{API_BASE}/leaderboards/{}/category/{}?top=1&embed=players",
            percent_encode(&self.lookup.game_id),
            percent_encode(&self.lookup.category_id)
        );
        if query.filter_platform && !self.lookup.platform_id.is_empty() {
            push_parameter(&mut url, "platform", &self.lookup.platform_id);
            push_parameter(
                &mut url,
                "emulators",
                if query.uses_emulator { "1" } else { "0" },
            );
        }
        if query.filter_region && !self.lookup.region_id.is_empty() {
            push_parameter(&mut url, "region", &self.lookup.region_id);
        }
        for (variable, value) in &self.lookup.variable_filters {
            push_parameter(&mut url, &format!("var-{variable}"), value);
        }
        if let Some(method) = query.timing_method {
            push_parameter(
                &mut url,
                "timing",
                match method {
                    TimingMethodOverride::RealTime => "realtime",
                    TimingMethodOverride::RealTimeWithoutLoads => "realtime_noloads",
                    TimingMethodOverride::GameTime => "ingame",
                },
            );
        }
        url
    }

    fn sync_query(&mut self, timer: &Snapshot<'_>) {
        let run = timer.run();
        let metadata = run.metadata();
        let query = Query {
            game: run.game_name().into(),
            category: run.category_name().into(),
            platform: metadata.platform_name().into(),
            region: metadata.region_name().into(),
            uses_emulator: metadata.uses_emulator(),
            variables: metadata
                .speedrun_com_variables()
                .map(|(name, value)| (name.to_owned(), value.clone()))
                .collect(),
            filter_variables: self.settings.filter_variables,
            filter_subcategories: self.settings.filter_subcategories,
            filter_platform: self.settings.filter_platform,
            filter_region: self.settings.filter_region,
            timing_method: self.settings.timing_method,
        };

        let expired = self.lookup.completed_at.is_some_and(|completed_at| {
            (TimeStamp::now() - completed_at) >= TimeSpan::from_seconds(REFRESH_SECONDS)
        });
        if self.lookup.query.as_ref() != Some(&query) || expired {
            self.lookup = Lookup {
                query: Some(query),
                ..Lookup::default()
            };
        }
    }

    /// Updates the component's visual state.
    pub fn update_state(&mut self, state: &mut key_value::State, timer: &Snapshot<'_>, lang: Lang) {
        self.sync_query(timer);
        state.background = self.settings.background;
        state.key_color = self.settings.label_color;
        state.value_color = self.settings.value_color;
        state.semantic_color = Default::default();
        state.key.clear();
        state.key.push_str(self.name());
        state.value.clear();

        let record_time = self
            .lookup
            .records
            .first()
            .and_then(|record| record.time(self.settings.timing_method));
        if let Some(mut seconds) = record_time {
            let accuracy = self.accuracy(seconds);
            let mut runners = self.lookup.records.first().unwrap().runners.join(" & ");
            let mut tie_count = self.lookup.records.len();

            let method =
                self.settings
                    .timing_method
                    .map(|method| match method {
                        TimingMethodOverride::RealTime => TimingMethod::RealTime,
                        TimingMethodOverride::RealTimeWithoutLoads
                        | TimingMethodOverride::GameTime => TimingMethod::GameTime,
                    })
                    .unwrap_or_else(|| timer.current_timing_method());
            if let Some(pb) = local_pb(timer, method) {
                let factor = if accuracy == Accuracy::Seconds {
                    1.0
                } else {
                    1000.0
                };
                if (pb.total_seconds() * factor) as i64 <= (seconds * factor) as i64 {
                    seconds = pb.total_seconds();
                    runners = "me".into();
                    tie_count = 1;
                }
            }

            let _ = write!(
                state.value,
                "{}",
                Regular::with_accuracy(accuracy)
                    .format(Some(TimeSpan::from_seconds(seconds)), lang)
            );
            if tie_count > 1 {
                let _ = write!(state.value, " ({tie_count}-way tie)");
            } else if !runners.is_empty() {
                let _ = write!(state.value, " by {runners}");
            }
        } else if self.lookup.stage != Stage::Complete && self.lookup.error.is_none() {
            state.value.push_str("Loading...");
        } else {
            state.value.push('-');
        }

        state.key_abbreviations.clear();
        state.key_abbreviations.push("WR".into());
        state.display_two_rows = self.settings.display_two_rows;
        state.updates_frequently = false;
    }

    fn accuracy(&self, seconds: f64) -> Accuracy {
        if self.settings.automatic_precision {
            if TimeSpan::from_seconds(seconds)
                .to_seconds_and_subsec_nanoseconds()
                .1
                == 0
            {
                Accuracy::Seconds
            } else {
                Accuracy::Milliseconds
            }
        } else {
            self.settings.accuracy
        }
    }

    /// Calculates the component's visual state.
    pub fn state(&mut self, timer: &Snapshot<'_>, lang: Lang) -> key_value::State {
        let mut state = key_value::State::default();
        self.update_state(&mut state, timer, lang);
        state
    }

    /// Accesses a generic description of the component's settings.
    pub fn settings_description(&self, _lang: Lang) -> SettingsDescription {
        SettingsDescription::with_fields(vec![
            field(
                "Background",
                "The component background.",
                self.settings.background.into(),
            ),
            field(
                "Display 2 Rows",
                "Displays the label and value on separate rows.",
                self.settings.display_two_rows.into(),
            ),
            field(
                "Filter Variables",
                "Filters to non-subcategory variables from the splits.",
                self.settings.filter_variables.into(),
            ),
            field(
                "Filter Subcategories",
                "Filters to subcategory variables from the splits.",
                self.settings.filter_subcategories.into(),
            ),
            field(
                "Filter Platform",
                "Filters to the platform and emulator usage from the splits.",
                self.settings.filter_platform.into(),
            ),
            field(
                "Filter Region",
                "Filters to the region from the splits.",
                self.settings.filter_region.into(),
            ),
            field(
                "Timing Method",
                "Overrides the leaderboard's primary timing method.",
                timing_method_name(self.settings.timing_method)
                    .to_owned()
                    .into(),
            ),
            field(
                "Automatic Precision",
                "Uses the precision shown on the leaderboard.",
                self.settings.automatic_precision.into(),
            ),
            field(
                "Accuracy",
                "The time accuracy when automatic precision is disabled.",
                self.settings.accuracy.into(),
            ),
            field(
                "Label Color",
                "The color of the label.",
                self.settings.label_color.into(),
            ),
            field(
                "Value Color",
                "The color of the value.",
                self.settings.value_color.into(),
            ),
        ])
    }

    /// Sets a setting by its index in the settings description.
    pub fn set_value(&mut self, index: usize, value: Value) {
        match index {
            0 => self.settings.background = value.into(),
            1 => self.settings.display_two_rows = value.into(),
            2 => self.settings.filter_variables = value.into(),
            3 => self.settings.filter_subcategories = value.into(),
            4 => self.settings.filter_platform = value.into(),
            5 => self.settings.filter_region = value.into(),
            6 => {
                let value: String = value.into();
                self.settings.timing_method = parse_timing_method(&value);
            }
            7 => self.settings.automatic_precision = value.into(),
            8 => self.settings.accuracy = value.into(),
            9 => self.settings.label_color = value.into(),
            10 => self.settings.value_color = value.into(),
            _ => panic!("Unsupported Setting Index"),
        }
    }
}

fn local_pb(timer: &Snapshot<'_>, method: TimingMethod) -> Option<TimeSpan> {
    let last = timer.run().segments().last()?;
    let pb = last.personal_best_split_time()[method];
    let split = last.split_time()[method];
    if timer.current_phase() == TimerPhase::Ended && split < pb {
        split
    } else {
        pb
    }
}

fn timing_method_name(method: Option<TimingMethodOverride>) -> &'static str {
    match method {
        None => "Default for Leaderboard",
        Some(TimingMethodOverride::RealTime) => "Real Time",
        Some(TimingMethodOverride::RealTimeWithoutLoads) => "Real Time Without Loads",
        Some(TimingMethodOverride::GameTime) => "Game Time",
    }
}

fn parse_timing_method(value: &str) -> Option<TimingMethodOverride> {
    match value {
        "Real Time" => Some(TimingMethodOverride::RealTime),
        "Real Time Without Loads" => Some(TimingMethodOverride::RealTimeWithoutLoads),
        "Game Time" => Some(TimingMethodOverride::GameTime),
        _ => None,
    }
}

fn resolve_player(
    player: &serde_json::Value,
    embedded: Option<&Vec<serde_json::Value>>,
) -> Result<String, ParseResponseError> {
    if player.pointer("/rel").and_then(serde_json::Value::as_str) == Some("guest") {
        return Ok(required_str(player, "/name")?.into());
    }
    let id = required_str(player, "/id")?;
    Ok(embedded
        .and_then(|players| {
            players
                .iter()
                .find(|entry| optional_str(entry, "/id") == Some(id))
        })
        .and_then(|entry| optional_str(entry, "/names/international"))
        .unwrap_or(id)
        .into())
}

fn data_array(value: &serde_json::Value) -> Result<&[serde_json::Value], ParseResponseError> {
    value
        .pointer("/data")
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| invalid("The response has no data array."))
}

fn find_named<'a>(
    values: &'a [serde_json::Value],
    wanted: &str,
    pointer: &str,
) -> Option<&'a serde_json::Value> {
    values.iter().find(|value| {
        optional_str(value, pointer).is_some_and(|name| name.eq_ignore_ascii_case(wanted))
    })
}

fn required_str<'a>(
    value: &'a serde_json::Value,
    pointer: &str,
) -> Result<&'a str, ParseResponseError> {
    optional_str(value, pointer)
        .ok_or_else(|| invalid(format!("The response is missing {pointer}.")))
}

fn optional_str<'a>(value: &'a serde_json::Value, pointer: &str) -> Option<&'a str> {
    value.pointer(pointer).and_then(serde_json::Value::as_str)
}

fn optional_f64(value: &serde_json::Value, pointer: &str) -> Option<f64> {
    value.pointer(pointer).and_then(serde_json::Value::as_f64)
}

fn invalid(message: impl Into<String>) -> ParseResponseError {
    ParseResponseError::InvalidResponse {
        message: message.into(),
    }
}

fn field(name: &'static str, description: &'static str, value: Value) -> Field {
    Field::new(name.into(), description.into(), value)
}

fn push_parameter(url: &mut String, name: &str, value: &str) {
    let _ = write!(url, "&{}={}", percent_encode(name), percent_encode(value));
}

fn percent_encode(text: &str) -> String {
    let mut encoded = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                encoded.push(char::from(byte));
            }
            _ => {
                const HEX: &[u8; 16] = b"0123456789ABCDEF";
                encoded.push('%');
                encoded.push(char::from(HEX[usize::from(byte >> 4)]));
                encoded.push(char::from(HEX[usize::from(byte & 0x0f)]));
            }
        }
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Run, Segment, Timer};

    fn timer() -> Timer {
        let mut run = Run::new();
        run.set_game_name("Game & Watch");
        run.set_category_name("Any%");
        run.metadata_mut()
            .set_speedrun_com_variable("Glitches", "No");
        run.push_segment(Segment::new("End"));
        Timer::new(run).unwrap()
    }

    #[test]
    fn follows_the_current_run_and_resolves_ties() {
        let timer = timer();
        let mut component = Component::new();
        assert_eq!(
            component.request_url(&timer.snapshot()).as_deref(),
            Some("https://www.speedrun.com/api/v1/games?name=Game%20%26%20Watch")
        );
        component
            .parse_response(r#"{"data":[{"id":"g","names":{"international":"Game & Watch"}}]}"#)
            .unwrap();
        component
            .parse_response(r#"{"data":[{"id":"c","name":"Any%"}]}"#)
            .unwrap();
        component
            .parse_response(r#"{"data":[{"id":"v","name":"Glitches","is-subcategory":true,"values":{"choices":{"no":{"label":"No"}}}}]}"#)
            .unwrap();
        assert!(
            component
                .request_url(&timer.snapshot())
                .unwrap()
                .contains("var-v=no")
        );
        component
            .parse_response(r#"{"data":{"runs":[{"run":{"times":{"primary_t":65.23},"players":[{"rel":"user","id":"a"}]}},{"run":{"times":{"primary_t":65.23},"players":[{"rel":"guest","name":"Guest"}]}}],"players":{"data":[{"id":"a","names":{"international":"Runner"}}]}}}"#)
            .unwrap();

        assert_eq!(
            component.state(&timer.snapshot(), Lang::English).value,
            "1:05.230 (2-way tie)"
        );
    }

    #[test]
    fn shows_a_single_runner_and_automatic_precision() {
        let timer = timer();
        let mut component = Component::new();
        component.lookup.query = Some(Query::default());
        component.lookup.stage = Stage::Complete;
        component.lookup.records.push(Record {
            times: Times {
                primary: Some(10.0),
                ..Times::default()
            },
            runners: vec!["Guest".into()],
        });
        // Keep the synthetic lookup query synchronized with the timer.
        component.lookup.query = Some(component.query_for_test(&timer.snapshot()));
        assert_eq!(
            component.state(&timer.snapshot(), Lang::English).value,
            "0:10 by Guest"
        );
    }
}

#[cfg(test)]
impl Component {
    fn query_for_test(&self, timer: &Snapshot<'_>) -> Query {
        let run = timer.run();
        Query {
            game: run.game_name().into(),
            category: run.category_name().into(),
            variables: run
                .metadata()
                .speedrun_com_variables()
                .map(|(key, value)| (key.to_owned(), value.clone()))
                .collect(),
            filter_subcategories: true,
            ..Query::default()
        }
    }
}
