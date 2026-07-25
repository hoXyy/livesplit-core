//! The C API for the Reset Chance Component.

use super::{Json, output_vec};
use crate::{component::OwnedComponent, key_value_component_state::OwnedKeyValueComponentState};
use livesplit_core::{Lang, Timer, component::reset_chance::Component as ResetChanceComponent};

/// A Reset Chance Component owned by the C API caller.
pub type OwnedResetChanceComponent = Box<ResetChanceComponent>;

/// Creates a new Reset Chance Component.
#[unsafe(no_mangle)]
pub extern "C" fn ResetChanceComponent_new() -> OwnedResetChanceComponent {
    Box::new(ResetChanceComponent::new())
}

/// Drops a Reset Chance Component.
#[unsafe(no_mangle)]
pub extern "C" fn ResetChanceComponent_drop(this: OwnedResetChanceComponent) {
    drop(this);
}

/// Converts the component into a generic layout component.
#[unsafe(no_mangle)]
pub extern "C" fn ResetChanceComponent_into_generic(
    this: OwnedResetChanceComponent,
) -> OwnedComponent {
    Box::new((*this).into())
}

/// Encodes the component's state as JSON.
#[unsafe(no_mangle)]
pub extern "C" fn ResetChanceComponent_state_as_json(
    this: &ResetChanceComponent,
    timer: &Timer,
    lang: Lang,
) -> Json {
    output_vec(|output| {
        this.state(&timer.snapshot(), lang)
            .write_json(output)
            .unwrap();
    })
}

/// Calculates the component's state.
#[unsafe(no_mangle)]
pub extern "C" fn ResetChanceComponent_state(
    this: &ResetChanceComponent,
    timer: &Timer,
    lang: Lang,
) -> OwnedKeyValueComponentState {
    Box::new(this.state(&timer.snapshot(), lang))
}
