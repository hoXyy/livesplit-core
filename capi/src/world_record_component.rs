//! The C API for the World Record Component.

use crate::{
    Json, component::OwnedComponent, key_value_component_state::OwnedKeyValueComponentState,
    output_str, output_vec, str,
};
use livesplit_core::{Lang, Timer, component::world_record::Component as WorldRecordComponent};
use std::os::raw::c_char;

/// A World Record Component owned by the C API caller.
pub type OwnedWorldRecordComponent = Box<WorldRecordComponent>;

/// Creates a new World Record Component.
#[unsafe(no_mangle)]
pub extern "C" fn WorldRecordComponent_new() -> OwnedWorldRecordComponent {
    Box::new(WorldRecordComponent::new())
}

/// Drops a World Record Component.
#[unsafe(no_mangle)]
pub extern "C" fn WorldRecordComponent_drop(this: OwnedWorldRecordComponent) {
    drop(this);
}

/// Converts a World Record Component into a generic component.
#[unsafe(no_mangle)]
pub extern "C" fn WorldRecordComponent_into_generic(
    this: OwnedWorldRecordComponent,
) -> OwnedComponent {
    Box::new((*this).into())
}

/// Returns the URL for the next speedrun.com request, or an empty string if no
/// request is needed.
#[unsafe(no_mangle)]
pub extern "C" fn WorldRecordComponent_request_url(
    this: &mut WorldRecordComponent,
    timer: &Timer,
) -> *const c_char {
    output_str(this.request_url(&timer.snapshot()).unwrap_or_default())
}

/// Restarts the speedrun.com lookup to refresh the world record.
#[unsafe(no_mangle)]
pub extern "C" fn WorldRecordComponent_refresh(this: &mut WorldRecordComponent) {
    this.refresh();
}

/// Parses a response to the URL returned by
/// [`WorldRecordComponent_request_url`]. Returns whether parsing succeeded.
///
/// # Safety
///
/// `response` must be a valid null-terminated UTF-8 string.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn WorldRecordComponent_parse_response(
    this: &mut WorldRecordComponent,
    response: *const c_char,
) -> bool {
    // SAFETY: The caller guarantees that `response` is a valid string.
    this.parse_response(unsafe { str(response) }).is_ok()
}

/// Calculates the component's state.
#[unsafe(no_mangle)]
pub extern "C" fn WorldRecordComponent_state(
    this: &mut WorldRecordComponent,
    timer: &Timer,
    lang: Lang,
) -> OwnedKeyValueComponentState {
    Box::new(this.state(&timer.snapshot(), lang))
}

/// Encodes the component's state as JSON.
#[unsafe(no_mangle)]
pub extern "C" fn WorldRecordComponent_state_as_json(
    this: &mut WorldRecordComponent,
    timer: &Timer,
    lang: Lang,
) -> Json {
    output_vec(|output| {
        this.state(&timer.snapshot(), lang)
            .write_json(output)
            .unwrap();
    })
}
