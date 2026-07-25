use super::{GradientBuilder, Result, color, end_tag, parse_bool, parse_children};
use crate::{
    component::reset_chance::{CalculationBasis, ChanceMode, Component, PercentageAccuracy},
    util::xml::{Reader, helper::text_as_escaped_string_err},
};

pub fn settings(reader: &mut Reader, component: &mut Component) -> Result<()> {
    let settings = component.settings_mut();
    let mut background = GradientBuilder::new();
    let (mut override_label, mut override_value) = (false, false);

    parse_children(reader, |reader, tag, _| {
        if background.parse_background(reader, tag.name())? {
            return Ok(());
        }
        match tag.name() {
            "TextColor" => color(reader, |value| settings.label_color = Some(value)),
            "OverrideTextColor" => parse_bool(reader, |value| override_label = value),
            "ChanceColor" => color(reader, |value| settings.value_color = Some(value)),
            "OverrideChanceColor" => parse_bool(reader, |value| override_value = value),
            "ChanceMode" => text_as_escaped_string_err(reader, |value| {
                if let Some(value) = match value {
                    "ResetChance" => Some(ChanceMode::ResetChance),
                    "SuccessChance" => Some(ChanceMode::SuccessChance),
                    "RunsEnded" => Some(ChanceMode::RunsEnded),
                    _ => None,
                } {
                    settings.chance_mode = value;
                }
                Ok(())
            }),
            "Accuracy" => text_as_escaped_string_err(reader, |value| {
                if let Some(value) = match value {
                    "ZeroDecimal" => Some(PercentageAccuracy::Integer),
                    "OneDecimal" => Some(PercentageAccuracy::Tenths),
                    "TwoDecimal" => Some(PercentageAccuracy::Hundredths),
                    _ => None,
                } {
                    settings.accuracy = value;
                }
                Ok(())
            }),
            "ShowTrailingZeroes" => {
                parse_bool(reader, |value| settings.show_trailing_zeroes = value)
            }
            "Basis" => text_as_escaped_string_err(reader, |value| {
                if let Some(value) = match value {
                    "AllRuns" => Some(CalculationBasis::AllRuns),
                    "Subset" => Some(CalculationBasis::RecentRuns),
                    "SubsetSplits" => Some(CalculationBasis::RecentSplitAttempts),
                    _ => None,
                } {
                    settings.calculation_basis = value;
                }
                Ok(())
            }),
            "BasisSubset" => text_as_escaped_string_err(reader, |value| {
                if let Ok(value) = value.parse() {
                    settings.recent_runs = value;
                }
                Ok(())
            }),
            "BasisSubsetSplits" => text_as_escaped_string_err(reader, |value| {
                if let Ok(value) = value.parse() {
                    settings.recent_split_attempts = value;
                }
                Ok(())
            }),
            "Display2Rows" => parse_bool(reader, |value| settings.display_two_rows = value),
            _ => end_tag(reader),
        }
    })?;

    if !override_label {
        settings.label_color = None;
    }
    if !override_value {
        settings.value_color = None;
    }
    settings.background = background.build();
    Ok(())
}
