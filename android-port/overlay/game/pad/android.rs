//! Controllers on Android. winit has no gamepad API, and its Android backend
//! turns a pad's motion into phantom touches and drops its buttons; the patch
//! in `tools/patch-winit.sh` sends both out as `DeviceEvent`s instead — a
//! button as its Android key code, an axis by its id and value. This is where
//! they become the `PadState` the rest of Horizon already understands.
use super::{super::Game, Pads};
use horizon_client::gamepad::{Button, PadState};
use winit::event::{DeviceEvent, ElementState};

/// A pad's last state, kept from its events.
#[derive(Default)]
pub(super) struct AndroidPad {
    pub state: PadState,
    /// The D-pad's hat, `[x, y]` with right and up positive.
    hat: [f32; 2],
}

impl Pads {
    /// A pad button: an Android key code (`AKEYCODE_BUTTON_A` = 96 and so on).
    pub(super) fn android_button(&mut self, code: u32, down: bool) {
        let pad = &mut self.android;
        match code {
            // The triggers are analog on most pads; when a pad sends them as
            // plain buttons, they still have to land in `triggers`.
            104 => pad.state.triggers[0] = if down { 1.0 } else { 0.0 },
            105 => pad.state.triggers[1] = if down { 1.0 } else { 0.0 },
            _ => {
                if let Some(button) = button(code) {
                    pad.state.set(button, down);
                }
            }
        }
    }

    /// A pad axis, by its Android id (`AMOTION_EVENT_AXIS_*`).
    pub(super) fn android_axis(&mut self, axis: u32, value: f64) {
        let value = value as f32;
        let pad = &mut self.android;
        match axis {
            0 => pad.state.left[0] = value,      // AXIS_X
            1 => pad.state.left[1] = -value,     // AXIS_Y: Android's y grows down
            11 => pad.state.right[0] = value,    // AXIS_Z
            14 => pad.state.right[1] = -value,   // AXIS_RZ
            15 => {
                pad.hat[0] = value; // AXIS_HAT_X
                hat(&mut pad.state, pad.hat);
            }
            16 => {
                pad.hat[1] = -value; // AXIS_HAT_Y
                hat(&mut pad.state, pad.hat);
            }
            17 => pad.state.triggers[0] = value.clamp(0.0, 1.0), // AXIS_LTRIGGER
            18 => pad.state.triggers[1] = value.clamp(0.0, 1.0), // AXIS_RTRIGGER
            _ => {}
        }
    }
}

fn hat(state: &mut PadState, [x, y]: [f32; 2]) {
    const ON: f32 = 0.5;
    state.set(Button::DPadLeft, x < -ON);
    state.set(Button::DPadRight, x > ON);
    state.set(Button::DPadUp, y > ON);
    state.set(Button::DPadDown, y < -ON);
}

fn button(code: u32) -> Option<Button> {
    Some(match code {
        96 => Button::South,
        97 => Button::East,
        99 => Button::West,
        100 => Button::North,
        102 => Button::LeftBumper,
        103 => Button::RightBumper,
        106 => Button::LeftStick,
        107 => Button::RightStick,
        108 => Button::Start,
        109 => Button::Select,
        19 => Button::DPadUp,
        20 => Button::DPadDown,
        21 => Button::DPadLeft,
        22 => Button::DPadRight,
        _ => return None,
    })
}

impl Game<'_> {
    /// A raw device event from the patched Android backend, when it is one of
    /// the pad's. Returns whether it was consumed.
    pub(in crate::game) fn pad_device_event(&mut self, event: &DeviceEvent) -> bool {
        match event {
            DeviceEvent::Button { button, state } => {
                self.pad.android_button(*button, *state == ElementState::Pressed);
                self.pad.in_use = true;
                true
            }
            DeviceEvent::Motion { axis, value } => {
                self.pad.android_axis(*axis, *value);
                true
            }
            _ => false,
        }
    }
}
