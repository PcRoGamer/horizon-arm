//! Independent, testable recognition of touch-to-interact gestures.
//! Behaviour is specified by observed Bedrock touch interactions and publicly
//! documented controls, not translated game code.

use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    Tap { x: f32, y: f32 },
    HoldBegin { x: f32, y: f32 },
    HoldEnd,
    Look { dx: f32, dy: f32 },
    // Android-only gestures are consumed by the game. Do not expose these to
    // desktop tests, where dead-code warnings are treated as errors.
    #[cfg(target_os = "android")]
    Hotbar(usize),
    #[cfg(target_os = "android")]
    Chat,
    #[cfg(target_os = "android")]
    Perspective,
}

/// Hit test the actual vanilla hotbar, independent of device resolution.
/// The HUD draws 9 slots, each 20 GUI pixels wide, centred on the window,
/// with the bottommost 22 GUI pixels occupied by the hotbar background.
pub const HOTBAR_SCALE: f32 = 1.36;
pub const HOTBAR_BOTTOM: f32 = 15.0;
pub const INVENTORY_CAP: f32 = 20.0;

/// Same minimum-width safety clamp as game/touch_hotbar.rs.
/// Hitboxes must follow rendered vertices, not an independent preset.
pub fn hotbar_scale(width: f32) -> f32 {
    HOTBAR_SCALE.min((width * 0.5 - 10.0) / (91.0 + INVENTORY_CAP))
}

/// Pure sprint state machine: thumb-lock or always-forward sprint, never
/// driven by unreliable double-tap movement gestures.
pub fn sprint_engaged(forward: f32, locked: bool, always: bool) -> bool {
    forward > 0.18 && (locked || (always && forward > 0.55))
}

/// The actual *transformed* native nine-slot bar. GUI coords, not physical
/// screen coords. Must agree with game/touch_hotbar.rs for every GUI scale.
pub fn hotbar_slot(normalized: [f32; 2], gui: [u32; 2]) -> Option<usize> {
    let [width, height] = gui;
    if width < 320 || height < 200 {
        return None;
    }
    let w = width as f32;
    let h = height as f32;
    let x = normalized[0] * w;
    let y = normalized[1] * h;
    let s = hotbar_scale(w);
    let left = w * 0.5 - 90.0 * s;
    let top = h - HOTBAR_BOTTOM - 22.0 * s;
    if x < left || x >= left + 180.0 * s || y < top || y > h - HOTBAR_BOTTOM {
        return None;
    }
    Some(((x - left) / (20.0 * s)) as usize)
}

/// Right-edge inventory endcap is physically part of the native hotbar.
pub fn hotbar_inventory(normalized: [f32; 2], gui: [u32; 2]) -> bool {
    let [width, height] = gui;
    if width < 320 || height < 200 {
        return false;
    }
    let w = width as f32;
    let h = height as f32;
    let x = normalized[0] * w;
    let y = normalized[1] * h;
    let s = hotbar_scale(w);
    let left = w * 0.5 + 91.0 * s - 1.0;
    let top = h - HOTBAR_BOTTOM - 22.0 * s;
    x >= left && x < left + INVENTORY_CAP * s && y >= top && y <= h - HOTBAR_BOTTOM
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Press,
    Drag,
    Hold,
}

#[derive(Debug, Clone, Copy)]
struct Contact {
    start: [f32; 2],
    previous: [f32; 2],
    since_ms: u64,
    phase: Phase,
}

/// The view gesture owns exactly one finger; joystick and action buttons
/// retain separate IDs in the Android input adapter.
#[derive(Debug)]
pub struct Recognizer {
    view: Option<u64>,
    contacts: BTreeMap<u64, Contact>,
    hold_ms: u64,
    slop_px: f32,
}

impl Recognizer {
    pub fn new(screen_height_px: f32) -> Self {
        Self {
            view: None,
            contacts: BTreeMap::new(),
            hold_ms: 350,
            // Physical pixels, not a fraction of the screen's width. This
            // preserves tap/drag discrimination on very wide phones.
            slop_px: (screen_height_px * 0.018).clamp(10.0, 26.0),
        }
    }

    pub fn set_screen_height(&mut self, screen_height_px: f32) {
        self.slop_px = (screen_height_px * 0.018).clamp(10.0, 26.0);
    }

    /// Returns false when another view finger already owns camera input.
    pub fn down(&mut self, id: u64, x: f32, y: f32, now_ms: u64) -> bool {
        if self.view.is_some() {
            return false;
        }
        self.view = Some(id);
        self.contacts.insert(
            id,
            Contact {
                start: [x, y],
                previous: [x, y],
                since_ms: now_ms,
                phase: Phase::Press,
            },
        );
        true
    }

    pub fn move_to(&mut self, id: u64, x: f32, y: f32) -> Option<Action> {
        if self.view != Some(id) {
            return None;
        }
        let contact = self.contacts.get_mut(&id)?;
        let dx = x - contact.previous[0];
        let dy = y - contact.previous[1];
        let moved_x = x - contact.start[0];
        let moved_y = y - contact.start[1];
        if contact.phase == Phase::Press
            && moved_x.mul_add(moved_x, moved_y * moved_y) > self.slop_px * self.slop_px
        {
            contact.phase = Phase::Drag;
        }
        contact.previous = [x, y];
        (contact.phase == Phase::Drag && (dx != 0.0 || dy != 0.0))
            .then_some(Action::Look { dx, dy })
    }

    /// Called each game-frame even when Android sends no further move event.
    /// A still finger must begin mining after the hold threshold.
    pub fn tick(&mut self, now_ms: u64) -> Option<Action> {
        let contact = self.contacts.get_mut(&self.view?)?;
        if contact.phase == Phase::Press && now_ms.saturating_sub(contact.since_ms) >= self.hold_ms
        {
            contact.phase = Phase::Hold;
            return Some(Action::HoldBegin {
                x: contact.previous[0],
                y: contact.previous[1],
            });
        }
        None
    }

    pub fn up(&mut self, id: u64, x: f32, y: f32, cancelled: bool) -> Option<Action> {
        if self.view != Some(id) {
            self.contacts.remove(&id);
            return None;
        }
        self.view = None;
        let contact = self.contacts.remove(&id)?;
        if cancelled {
            return (contact.phase == Phase::Hold).then_some(Action::HoldEnd);
        }
        match contact.phase {
            Phase::Press => {
                let dx = x - contact.start[0];
                let dy = y - contact.start[1];
                (dx.mul_add(dx, dy * dy) <= self.slop_px * self.slop_px)
                    .then_some(Action::Tap { x, y })
            }
            Phase::Hold => Some(Action::HoldEnd),
            Phase::Drag => None,
        }
    }

    /// A pause, focus loss or activity switch cancels holds without firing taps.
    pub fn reset(&mut self) -> Option<Action> {
        let holding = self.contacts.values().any(|c| c.phase == Phase::Hold);
        self.contacts.clear();
        self.view = None;
        holding.then_some(Action::HoldEnd)
    }
}

#[cfg(test)]
mod tests {
    use super::{Action, Recognizer};

    #[test]
    fn quick_tap_is_an_interaction_not_camera_motion() {
        let mut r = Recognizer::new(1080.0);
        assert!(r.down(1, 600.0, 420.0, 0));
        assert_eq!(r.tick(200), None);
        assert_eq!(r.move_to(1, 605.0, 424.0), None);
        assert_eq!(
            r.up(1, 605.0, 424.0, false),
            Some(Action::Tap { x: 605.0, y: 424.0 })
        );
    }

    #[test]
    fn long_hold_starts_once_and_releases() {
        let mut r = Recognizer::new(1080.0);
        r.down(1, 600.0, 420.0, 10);
        assert_eq!(r.tick(359), None);
        assert_eq!(r.tick(360), Some(Action::HoldBegin { x: 600.0, y: 420.0 }));
        assert_eq!(r.tick(700), None);
        assert_eq!(r.up(1, 600.0, 420.0, false), Some(Action::HoldEnd));
    }

    #[test]
    fn swipe_never_becomes_a_tap_or_hold() {
        let mut r = Recognizer::new(1080.0);
        r.down(1, 600.0, 420.0, 0);
        assert_eq!(
            r.move_to(1, 630.0, 440.0),
            Some(Action::Look { dx: 30.0, dy: 20.0 })
        );
        assert_eq!(r.tick(1000), None);
        assert_eq!(r.up(1, 630.0, 440.0, false), None);
    }

    #[test]
    fn second_view_finger_cannot_steal_or_generate_action() {
        let mut r = Recognizer::new(1080.0);
        assert!(r.down(4, 600.0, 400.0, 0));
        assert!(!r.down(5, 700.0, 450.0, 10));
        assert_eq!(r.up(5, 700.0, 450.0, false), None);
        assert_eq!(
            r.up(4, 600.0, 400.0, false),
            Some(Action::Tap { x: 600.0, y: 400.0 })
        );
    }

    #[test]
    fn cancelled_and_lost_focus_release_but_do_not_tap() {
        let mut r = Recognizer::new(1080.0);
        r.down(1, 600.0, 400.0, 0);
        assert_eq!(r.up(1, 600.0, 400.0, true), None);
        r.down(2, 600.0, 400.0, 0);
        assert!(matches!(r.tick(400), Some(Action::HoldBegin { .. })));
        assert_eq!(r.reset(), Some(Action::HoldEnd));
        assert_eq!(r.tick(600), None);
    }

    #[test]
    fn scaled_and_raised_hotbar_has_exactly_nine_slots_and_attached_inventory() {
        use super::{HOTBAR_BOTTOM, INVENTORY_CAP, hotbar_inventory, hotbar_scale};
        for gui in [[320, 240], [480, 270], [960, 540], [320, 200], [426, 240]] {
            let [w, h] = gui.map(|v| v as f32);
            let s = hotbar_scale(w);
            let left = w * 0.5 - 90.0 * s;
            let y = (h - HOTBAR_BOTTOM - 11.0 * s) / h;
            for i in 0..9 {
                let nx = (left + (10.0 + 20.0 * i as f32) * s) / w;
                assert_eq!(super::hotbar_slot([nx, y], gui), Some(i));
                assert!(!hotbar_inventory([nx, y], gui));
            }
            assert_eq!(super::hotbar_slot([(left - 1.0) / w, y], gui), None);
            assert_eq!(super::hotbar_slot([0.5, 0.5], gui), None);
            let inventory_x = (w * 0.5 + (91.0 + INVENTORY_CAP / 2.0) * s - 1.0) / w;
            assert!(hotbar_inventory([inventory_x, y], gui));
            assert_eq!(super::hotbar_slot([inventory_x, y], gui), None);
            assert!(!hotbar_inventory([inventory_x, 0.98], gui));
        }
    }

    #[test]
    fn sprint_only_engages_forward_and_unlocks_cleanly() {
        use super::sprint_engaged as sprint;
        assert!(!sprint(0.0, false, false));
        assert!(!sprint(-1.0, true, true));
        assert!(!sprint(0.18, true, true));
        assert!(sprint(0.19, true, false));
        assert!(!sprint(0.55, false, true));
        assert!(sprint(0.56, false, true));
        assert!(sprint(0.9, true, true));
        assert!(!sprint(0.9, false, false));
    }

    #[test]
    fn thresholds_scale_without_exploding_on_tablets() {
        assert_eq!(Recognizer::new(360.0).slop_px, 10.0);
        assert_eq!(Recognizer::new(2160.0).slop_px, 26.0);
        let mut recognizer = Recognizer::new(360.0);
        recognizer.set_screen_height(2160.0);
        assert_eq!(recognizer.slop_px, 26.0);
    }
}
