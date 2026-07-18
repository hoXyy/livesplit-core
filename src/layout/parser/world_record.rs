use super::{GradientBuilder, Result, color, end_tag, parse_bool, parse_children};
use crate::{
    component::world_record::{Component, TimingMethodOverride},
    timing::formatter::Accuracy,
    util::xml::{Reader, helper::text_as_escaped_string_err},
};

pub fn settings(reader: &mut Reader, component: &mut Component) -> Result<()> {
    let settings = component.settings_mut();
    let mut background_builder = GradientBuilder::new();
    let (mut override_label, mut override_value) = (false, false);

    parse_children(reader, |reader, tag, _| {
        if !background_builder.parse_background(reader, tag.name())? {
            match tag.name() {
                "TextColor" => color(reader, |color| settings.label_color = Some(color)),
                "OverrideTextColor" => parse_bool(reader, |value| override_label = value),
                "TimeColor" => color(reader, |color| settings.value_color = Some(color)),
                "OverrideTimeColor" => parse_bool(reader, |value| override_value = value),
                "Display2Rows" => parse_bool(reader, |value| settings.display_two_rows = value),
                "FilterRegion" => parse_bool(reader, |value| settings.filter_region = value),
                "FilterPlatform" => parse_bool(reader, |value| settings.filter_platform = value),
                "FilterVariables" => parse_bool(reader, |value| settings.filter_variables = value),
                "FilterSubcategories" => {
                    parse_bool(reader, |value| settings.filter_subcategories = value)
                }
                "TimingMethod" => text_as_escaped_string_err(reader, |value| {
                    settings.timing_method = match value {
                        "Real Time" => Some(TimingMethodOverride::RealTime),
                        "Real Time Without Loads" => {
                            Some(TimingMethodOverride::RealTimeWithoutLoads)
                        }
                        "Game Time" => Some(TimingMethodOverride::GameTime),
                        _ => None,
                    };
                    Ok(())
                }),
                "PrecisionType" => text_as_escaped_string_err(reader, |value| {
                    settings.automatic_precision = value == "FromLeaderboard";
                    settings.accuracy = match value {
                        "Seconds" => Accuracy::Seconds,
                        _ => Accuracy::Milliseconds,
                    };
                    Ok(())
                }),
                _ => end_tag(reader),
            }
        } else {
            Ok(())
        }
    })?;

    if !override_label {
        settings.label_color = None;
    }
    if !override_value {
        settings.value_color = None;
    }
    settings.background = background_builder.build();
    Ok(())
}
