//! Translating winit input into `noonoo-events`.
//!
//! This module is deliberately **pure**: it takes a [`WindowEvent`] plus the
//! current scale factor and modifier state, and returns the [`Action`] the host
//! should perform — or `None` for events Noonoo does not model. That makes the
//! platform seam testable without a display, which is exactly where the risk
//! of a beta winit API lives.
//!
//! Conventions:
//!
//! - Coordinates are converted from physical to **logical** pixels (divided by
//!   the scale factor), because Noonoo's layout and hit-testing work in logical
//!   pixels.
//! - Wheel deltas are inverted: winit reports "content moves down" for a
//!   positive delta, while `Host::wheel`'s positive delta scrolls *forward*
//!   (content moves up).
//! - `NamedKey`s map to their Debug name (`"Enter"`, `"ArrowUp"`, …);
//!   characters map to their text.

use noonoo_events::{ElementState, Event, KeyEvent, Modifiers, MouseButton, PointerEvent};
use noonoo_geometry::{Point, Size};
use winit::event::{
    ButtonSource, ElementState as WinitState, MouseButton as WinitButton, MouseScrollDelta,
    WindowEvent,
};
use winit::keyboard::{Key as WinitKeyKind, ModifiersState};

/// Logical pixels one wheel `LineDelta` unit scrolls. Browsers converge on
/// roughly 40 px per line; the exact value is a host choice.
pub const LINE_HEIGHT: f64 = 40.0;

/// What a translated window event asks the host to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// Dispatch a pointer event at `point` (window coordinates, logical px).
    Pointer {
        /// Where the pointer is.
        point: Point,
        /// The event to dispatch.
        event: Event,
    },
    /// Dispatch a key event to the focused node.
    Key(KeyEvent),
    /// Scroll by `delta_y` (positive scrolls forward) at the current pointer
    /// position; the handler anchors the wheel on its own cursor.
    Wheel {
        /// The vertical delta.
        delta_y: f32,
    },
    /// The window's logical size changed.
    Resized(Size),
    /// The window asks for a frame.
    Redraw,
    /// The user asked to close the window.
    Close,
}

/// Translate a window event.
///
/// `scale` is the current device-pixel ratio; `modifiers` the last
/// `ModifiersChanged` state.
pub fn translate(event: &WindowEvent, scale: f64, modifiers: ModifiersState) -> Option<Action> {
    match event {
        WindowEvent::SurfaceResized(size) => Some(Action::Resized(logical_size(*size, scale))),
        WindowEvent::CloseRequested => Some(Action::Close),
        WindowEvent::RedrawRequested => Some(Action::Redraw),
        WindowEvent::PointerMoved { position, .. } => {
            let point = logical_point(*position, scale);
            Some(Action::Pointer {
                point,
                event: Event::PointerMove(pointer_event(
                    point,
                    None,
                    WinitState::Released,
                    modifiers,
                )),
            })
        }
        WindowEvent::PointerButton {
            state,
            position,
            button,
            ..
        } => {
            let point = logical_point(*position, scale);
            let button = mouse_button(button)?;
            let event = match state {
                WinitState::Pressed => {
                    Event::PointerDown(pointer_event(point, Some(button), *state, modifiers))
                }
                WinitState::Released => {
                    Event::PointerUp(pointer_event(point, Some(button), *state, modifiers))
                }
            };
            Some(Action::Pointer { point, event })
        }
        WindowEvent::MouseWheel { delta, .. } => {
            // The horizontal component is dropped: per-box horizontal wheel
            // input is not modelled yet.
            let (_, delta_y) = wheel_delta(*delta, scale);
            Some(Action::Wheel {
                delta_y: delta_y as f32,
            })
        }
        WindowEvent::KeyboardInput { event, .. } => {
            let (key, state) = key(&event.logical_key, event.state)?;
            Some(Action::Key(KeyEvent {
                key,
                state,
                modifiers: modifiers_of(modifiers),
            }))
        }
        _ => None,
    }
}

/// Physical pixels → logical [`Point`].
pub fn logical_point(position: winit::dpi::PhysicalPosition<f64>, scale: f64) -> Point {
    Point::new((position.x / scale) as f32, (position.y / scale) as f32)
}

/// Physical pixels → logical [`Size`].
pub fn logical_size(size: winit::dpi::PhysicalSize<u32>, scale: f64) -> Size {
    Size::new(
        (f64::from(size.width) / scale) as f32,
        (f64::from(size.height) / scale) as f32,
    )
}

/// A pointer event in Noonoo's model. A move carries no button (`Primary`
/// released); a press/release carries the button it belongs to.
fn pointer_event(
    point: Point,
    button: Option<MouseButton>,
    state: WinitState,
    modifiers: ModifiersState,
) -> PointerEvent {
    PointerEvent {
        x: f64::from(point.x),
        y: f64::from(point.y),
        button: button.unwrap_or(MouseButton::Primary),
        state: match state {
            WinitState::Pressed => ElementState::Pressed,
            WinitState::Released => ElementState::Released,
        },
        modifiers: modifiers_of(modifiers),
    }
}

/// A winit mouse button, or `None` for non-mouse pointers (touch, tablets).
fn mouse_button(button: &ButtonSource) -> Option<MouseButton> {
    match button {
        ButtonSource::Mouse(button) => Some(match button {
            WinitButton::Left => MouseButton::Primary,
            WinitButton::Right => MouseButton::Secondary,
            WinitButton::Middle => MouseButton::Middle,
            WinitButton::Back
            | WinitButton::Forward
            | WinitButton::Button6
            | WinitButton::Button7 => MouseButton::Other(*button as u16),
            _ => MouseButton::Other(*button as u16),
        }),
        _ => None,
    }
}

/// A wheel delta in **logical pixels**, inverted to Noonoo's convention
/// (positive scrolls forward).
fn wheel_delta(delta: MouseScrollDelta, scale: f64) -> (f64, f64) {
    match delta {
        MouseScrollDelta::LineDelta(x, y) => (-(x as f64) * LINE_HEIGHT, -y as f64 * LINE_HEIGHT),
        MouseScrollDelta::PixelDelta(position) => (-position.x / scale, -position.y / scale),
    }
}

/// A key name and state, or `None` for keys Noonoo cannot name (dead keys,
/// unidentified media keys).
fn key(key: &WinitKeyKind, state: WinitState) -> Option<(String, ElementState)> {
    // `NamedKey`'s `Display` is the spec name ("Enter", "ArrowDown", …);
    // space arrives as `Key::Character(" ")`, deliberately not a named key.
    let name = match key {
        WinitKeyKind::Character(text) => text.to_string(),
        WinitKeyKind::Named(named) => named.to_string(),
        WinitKeyKind::Dead(_) | WinitKeyKind::Unidentified(_) => return None,
    };
    let state = match state {
        WinitState::Pressed => ElementState::Pressed,
        WinitState::Released => ElementState::Released,
    };
    Some((name, state))
}

/// winit's modifier state into Noonoo's.
fn modifiers_of(modifiers: ModifiersState) -> Modifiers {
    Modifiers {
        alt: modifiers.alt_key(),
        ctrl: modifiers.control_key(),
        shift: modifiers.shift_key(),
        meta: modifiers.meta_key(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use noonoo_geometry::Size;
    use winit::dpi::{PhysicalPosition, PhysicalSize};
    use winit::event::KeyEvent as WinitKey;
    use winit::keyboard::{Key, KeyCode, ModifiersState as WinitModifiers, NamedKey, PhysicalKey};

    const SCALE: f64 = 2.0;

    fn mods() -> WinitModifiers {
        WinitModifiers::SHIFT | WinitModifiers::CONTROL
    }

    #[test]
    fn physical_positions_become_logical_points() {
        assert_eq!(
            logical_point(PhysicalPosition::new(200.0, 100.0), SCALE),
            Point::new(100.0, 50.0)
        );
        assert_eq!(
            logical_size(PhysicalSize::new(800, 600), SCALE),
            Size::new(400.0, 300.0)
        );
    }

    #[test]
    fn a_press_becomes_a_pointer_down() {
        let event = WindowEvent::PointerButton {
            device_id: None,
            state: WinitState::Pressed,
            position: PhysicalPosition::new(400.0, 300.0),
            primary: true,
            button: ButtonSource::Mouse(WinitButton::Left),
        };
        let Some(Action::Pointer { point, event }) = translate(&event, SCALE, mods()) else {
            panic!("expected a pointer action");
        };
        assert_eq!(point, Point::new(200.0, 150.0));
        let Event::PointerDown(pointer) = event else {
            panic!("expected a pointer down");
        };
        assert_eq!(pointer.button, MouseButton::Primary);
        assert_eq!(pointer.state, ElementState::Pressed);
        assert!(pointer.modifiers.shift && pointer.modifiers.ctrl);
        assert!(!pointer.modifiers.alt);
    }

    #[test]
    fn buttons_map_and_side_buttons_are_other() {
        let button_action = |button| {
            translate(
                &WindowEvent::PointerButton {
                    device_id: None,
                    state: WinitState::Released,
                    position: PhysicalPosition::new(0.0, 0.0),
                    primary: true,
                    button,
                },
                SCALE,
                WinitModifiers::empty(),
            )
        };
        let Some(Action::Pointer { event, .. }) =
            button_action(ButtonSource::Mouse(WinitButton::Right))
        else {
            panic!("expected a pointer action");
        };
        let Event::PointerUp(pointer) = event else {
            panic!("expected a pointer up");
        };
        assert_eq!(pointer.button, MouseButton::Secondary);

        // Side buttons still dispatch, mapped to `Other`.
        let Some(Action::Pointer { event, .. }) =
            button_action(ButtonSource::Mouse(WinitButton::Back))
        else {
            panic!("expected a pointer action");
        };
        let Event::PointerUp(pointer) = event else {
            panic!("expected a pointer up");
        };
        assert_eq!(pointer.button, MouseButton::Other(3));
    }

    #[test]
    fn a_wheel_line_scrolls_forward_by_the_line_height() {
        let event = WindowEvent::MouseWheel {
            device_id: None,
            delta: MouseScrollDelta::LineDelta(0.0, 1.0),
            phase: winit::event::TouchPhase::Started,
        };
        let Some(Action::Wheel { delta_y }) = translate(&event, SCALE, mods()) else {
            panic!("expected a wheel action");
        };
        // winit's positive delta means "content moves down"; Noonoo scrolls
        // forward, so the sign flips.
        assert_eq!(delta_y, -(LINE_HEIGHT as f32));
    }

    #[test]
    fn a_pixel_wheel_delta_scales_to_logical_pixels() {
        let event = WindowEvent::MouseWheel {
            device_id: None,
            delta: MouseScrollDelta::PixelDelta(PhysicalPosition::new(0.0, 120.0)),
            phase: winit::event::TouchPhase::Moved,
        };
        let Some(Action::Wheel { delta_y }) = translate(&event, SCALE, mods()) else {
            panic!("expected a wheel action");
        };
        assert_eq!(delta_y, -60.0);
    }

    fn winit_key(logical: Key, state: WinitState) -> WinitKey {
        WinitKey {
            physical_key: PhysicalKey::Code(KeyCode::KeyA),
            logical_key: logical.clone(),
            text: None,
            location: winit::keyboard::KeyLocation::Standard,
            state,
            repeat: false,
            text_with_all_modifiers: None,
            key_without_modifiers: logical,
        }
    }

    #[test]
    fn named_and_character_keys_map() {
        let event = WindowEvent::KeyboardInput {
            device_id: None,
            event: winit_key(Key::Named(NamedKey::Enter), WinitState::Pressed),
            is_synthetic: false,
        };
        let Some(Action::Key(key)) = translate(&event, SCALE, mods()) else {
            panic!("expected a key action");
        };
        assert_eq!(key.key, "Enter");
        assert_eq!(key.state, ElementState::Pressed);
        assert!(key.modifiers.shift);

        let event = WindowEvent::KeyboardInput {
            device_id: None,
            event: winit_key(Key::Character("a".into()), WinitState::Released),
            is_synthetic: false,
        };
        let Some(Action::Key(key)) = translate(&event, SCALE, mods()) else {
            panic!("expected a key action");
        };
        assert_eq!(key.key, "a");
        assert_eq!(key.state, ElementState::Released);
    }

    #[test]
    fn unnameable_keys_are_skipped() {
        let event = WindowEvent::KeyboardInput {
            device_id: None,
            event: winit_key(Key::Dead(None), WinitState::Pressed),
            is_synthetic: false,
        };
        assert_eq!(translate(&event, SCALE, mods()), None);
    }

    #[test]
    fn resize_close_and_redraw_map() {
        let resized = WindowEvent::SurfaceResized(PhysicalSize::new(1600, 1200));
        assert_eq!(
            translate(&resized, SCALE, mods()),
            Some(Action::Resized(Size::new(800.0, 600.0)))
        );
        assert_eq!(
            translate(&WindowEvent::CloseRequested, SCALE, mods()),
            Some(Action::Close)
        );
        assert_eq!(
            translate(&WindowEvent::RedrawRequested, SCALE, mods()),
            Some(Action::Redraw)
        );
        // Unmodelled events (focus changes, drag-and-drop, …) are ignored.
        assert_eq!(translate(&WindowEvent::Focused(true), SCALE, mods()), None);
    }
}
