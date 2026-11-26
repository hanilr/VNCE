//! Keyboard, mouse and gamepad state for fixed-timestep updates.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::time::{Duration, Instant};

use vnce_core::Position;
use vnce_term::input::{MouseButton, MouseEventKind};
use vnce_term::{
    Event, GamepadAxis, GamepadButton, GamepadEventKind, GamepadId, KeyCode, KeyEventKind,
    KeyModifiers,
};

/// Stick and trigger values closer to rest than this count as `0.0` in
/// [`InputState::axis`] (worn sticks rarely rest exactly at the center).
pub const DEAD_ZONE: f32 = 0.15;

/// A stick pushed this far (or a trigger pulled this far) counts as held for
/// [`InputState::is_stick_down`] and for stick bindings of actions.
pub const STICK_PRESS: f32 = 0.5;

/// A stick direction: an axis and its sign (`true` for the positive side).
type StickDirection = (GamepadAxis, bool);

/// A connected gamepad as seen by [`InputState`].
#[derive(Debug, Clone, PartialEq)]
pub struct Gamepad {
    id: GamepadId,
    name: String,
    buttons: HashSet<GamepadButton>,
    axes: [f32; 6],
}

impl Gamepad {
    /// The pad's id.
    #[must_use]
    pub fn id(&self) -> GamepadId {
        self.id
    }

    /// The pad's name as the system reports it.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns `true` while `button` is held down on this pad.
    #[must_use]
    pub fn is_down(&self, button: GamepadButton) -> bool {
        self.buttons.contains(&button)
    }

    /// The value of `axis` on this pad with the [`DEAD_ZONE`] applied.
    #[must_use]
    pub fn axis(&self, axis: GamepadAxis) -> f32 {
        apply_dead_zone(self.axes[axis.index()])
    }

    /// The value of `axis` as the pad reported it.
    #[must_use]
    pub fn axis_raw(&self, axis: GamepadAxis) -> f32 {
        self.axes[axis.index()]
    }
}

/// Which keys and mouse buttons are held down, and what changed since the previous update.
///
/// "Pressed" and "released" are reported in exactly one [`Game::update`](crate::Game)
/// call, even when several frames pass between two updates or several updates run in one
/// frame.
///
/// Character keys are case-insensitive: `Char('A')` and `Char('a')` are the same key, so
/// pressing or releasing Shift while a letter is held never leaves a key stuck.
///
/// On terminals that do not report key releases
/// ([`InputState::key_release_events`] is `false`), a key counts as released when no
/// press or repeat arrived for [`RunConfig::key_hold_timeout`](crate::RunConfig). Because
/// the operating system only repeats the most recently pressed key, holding two keys at
/// once is only reliable on terminals that report releases.
#[derive(Debug, Clone)]
pub struct InputState {
    key_release_events: bool,
    hold_timeout: Duration,
    /// Keys held down, with the time of their last press or repeat.
    down: HashMap<KeyCode, Instant>,
    /// Changes collected since the previous update.
    pending: Changes,
    /// Changes visible during the current update.
    current: Changes,
    modifiers: KeyModifiers,
    mouse_position: Option<Position>,
    mouse_down: [bool; 3],
    /// Connected gamepads by id.
    pads: BTreeMap<GamepadId, Gamepad>,
}

/// Changes between two updates.
#[derive(Debug, Clone, Default)]
struct Changes {
    pressed: HashSet<KeyCode>,
    released: HashSet<KeyCode>,
    mouse_pressed: [bool; 3],
    mouse_released: [bool; 3],
    scroll: (i32, i32),
    pad_pressed: HashSet<(GamepadId, GamepadButton)>,
    pad_released: HashSet<(GamepadId, GamepadButton)>,
    stick_pressed: HashSet<(GamepadId, StickDirection)>,
    stick_released: HashSet<(GamepadId, StickDirection)>,
}

impl Changes {
    fn clear(&mut self) {
        self.pressed.clear();
        self.released.clear();
        self.mouse_pressed = [false; 3];
        self.mouse_released = [false; 3];
        self.scroll = (0, 0);
        self.pad_pressed.clear();
        self.pad_released.clear();
        self.stick_pressed.clear();
        self.stick_released.clear();
    }
}

/// `value` with the dead zone cut out and the rest stretched back to the full range.
fn apply_dead_zone(value: f32) -> f32 {
    let size = value.abs();
    if size <= DEAD_ZONE || value.is_nan() {
        0.0
    } else {
        value.signum() * ((size - DEAD_ZONE) / (1.0 - DEAD_ZONE)).min(1.0)
    }
}

/// Whether `value` holds the stick direction `positive`.
fn holds(value: f32, positive: bool) -> bool {
    if positive {
        value >= STICK_PRESS
    } else {
        value <= -STICK_PRESS
    }
}

impl InputState {
    /// Creates an empty input state.
    #[must_use]
    pub fn new(key_release_events: bool, hold_timeout: Duration) -> Self {
        Self {
            key_release_events,
            hold_timeout,
            down: HashMap::new(),
            pending: Changes::default(),
            current: Changes::default(),
            modifiers: KeyModifiers::NONE,
            mouse_position: None,
            mouse_down: [false; 3],
            pads: BTreeMap::new(),
        }
    }

    /// Whether the terminal reports key releases. When `false`, releases are detected with
    /// a timeout.
    #[must_use]
    pub fn key_release_events(&self) -> bool {
        self.key_release_events
    }

    /// Returns `true` while `key` is held down.
    #[must_use]
    pub fn is_down(&self, key: KeyCode) -> bool {
        self.down.contains_key(&normalize(key))
    }

    /// Returns `true` if `key` was pressed since the previous update.
    #[must_use]
    pub fn was_pressed(&self, key: KeyCode) -> bool {
        self.current.pressed.contains(&normalize(key))
    }

    /// Returns `true` if `key` was released since the previous update.
    #[must_use]
    pub fn was_released(&self, key: KeyCode) -> bool {
        self.current.released.contains(&normalize(key))
    }

    /// Returns `true` if any of `keys` is held down.
    #[must_use]
    pub fn any_down(&self, keys: &[KeyCode]) -> bool {
        keys.iter().any(|&key| self.is_down(key))
    }

    /// All keys currently held down (characters in lowercase).
    pub fn keys_down(&self) -> impl Iterator<Item = KeyCode> + '_ {
        self.down.keys().copied()
    }

    /// Modifier keys of the most recent key or mouse event.
    #[must_use]
    pub fn modifiers(&self) -> KeyModifiers {
        self.modifiers
    }

    /// Last known mouse position, if the mouse was seen.
    #[must_use]
    pub fn mouse_position(&self) -> Option<Position> {
        self.mouse_position
    }

    /// Returns `true` while `button` is held down.
    #[must_use]
    pub fn is_mouse_down(&self, button: MouseButton) -> bool {
        self.mouse_down[button_index(button)]
    }

    /// Returns `true` if `button` was pressed since the previous update.
    #[must_use]
    pub fn was_mouse_pressed(&self, button: MouseButton) -> bool {
        self.current.mouse_pressed[button_index(button)]
    }

    /// Returns `true` if `button` was released since the previous update.
    #[must_use]
    pub fn was_mouse_released(&self, button: MouseButton) -> bool {
        self.current.mouse_released[button_index(button)]
    }

    /// Scroll since the previous update: `(horizontal, vertical)`, positive is right / down.
    #[must_use]
    pub fn scroll(&self) -> (i32, i32) {
        self.current.scroll
    }

    /// The connected gamepads, by id.
    pub fn gamepads(&self) -> impl Iterator<Item = &Gamepad> + '_ {
        self.pads.values()
    }

    /// The gamepad `id`, if it is connected.
    #[must_use]
    pub fn gamepad(&self, id: GamepadId) -> Option<&Gamepad> {
        self.pads.get(&id)
    }

    /// Returns `true` while `button` is held down on any gamepad.
    #[must_use]
    pub fn is_button_down(&self, button: GamepadButton) -> bool {
        self.pads.values().any(|pad| pad.is_down(button))
    }

    /// Returns `true` if `button` was pressed on any gamepad since the previous update.
    #[must_use]
    pub fn was_button_pressed(&self, button: GamepadButton) -> bool {
        self.current.pad_pressed.iter().any(|&(_, b)| b == button)
    }

    /// Returns `true` if `button` was released on any gamepad since the previous update.
    #[must_use]
    pub fn was_button_released(&self, button: GamepadButton) -> bool {
        self.current.pad_released.iter().any(|&(_, b)| b == button)
    }

    /// Returns `true` if `button` was pressed on pad `id` since the previous update (for
    /// games with a pad per player).
    #[must_use]
    pub fn was_pad_button_pressed(&self, id: GamepadId, button: GamepadButton) -> bool {
        self.current.pad_pressed.contains(&(id, button))
    }

    /// The value of `axis` with the [`DEAD_ZONE`] applied; with several pads, the one
    /// pushed furthest. `0.0` without pads.
    #[must_use]
    pub fn axis(&self, axis: GamepadAxis) -> f32 {
        self.pads
            .values()
            .map(|pad| pad.axis(axis))
            .fold(0.0, |best, v| if v.abs() > best.abs() { v } else { best })
    }

    /// Returns `true` while a stick (or trigger) of any pad is pushed past
    /// [`STICK_PRESS`] in the direction of `axis` (`positive`: right / up / pulled).
    #[must_use]
    pub fn is_stick_down(&self, axis: GamepadAxis, positive: bool) -> bool {
        self.pads
            .values()
            .any(|pad| holds(pad.axes[axis.index()], positive))
    }

    /// Returns `true` if a stick was pushed past [`STICK_PRESS`] in that direction since
    /// the previous update (a menu step per push).
    #[must_use]
    pub fn was_stick_pressed(&self, axis: GamepadAxis, positive: bool) -> bool {
        self.current
            .stick_pressed
            .iter()
            .any(|&(_, direction)| direction == (axis, positive))
    }

    /// Returns `true` if a stick came back from that direction since the previous update.
    #[must_use]
    pub fn was_stick_released(&self, axis: GamepadAxis, positive: bool) -> bool {
        self.current
            .stick_released
            .iter()
            .any(|&(_, direction)| direction == (axis, positive))
    }

    fn handle_gamepad(&mut self, id: GamepadId, kind: &GamepadEventKind) {
        match kind {
            GamepadEventKind::Connected(name) => {
                self.pads.entry(id).or_insert_with(|| Gamepad {
                    id,
                    name: name.clone(),
                    buttons: HashSet::new(),
                    axes: [0.0; 6],
                });
            }
            GamepadEventKind::Disconnected => {
                if let Some(pad) = self.pads.remove(&id) {
                    for button in pad.buttons {
                        self.pending.pad_released.insert((id, button));
                    }
                    for axis in GamepadAxis::ALL {
                        for positive in [false, true] {
                            if holds(pad.axes[axis.index()], positive) {
                                self.pending.stick_released.insert((id, (axis, positive)));
                            }
                        }
                    }
                }
            }
            GamepadEventKind::ButtonPressed(button) => {
                let pad = self.pad_entry(id);
                if pad.buttons.insert(*button) {
                    self.pending.pad_pressed.insert((id, *button));
                }
            }
            GamepadEventKind::ButtonReleased(button) => {
                let pad = self.pad_entry(id);
                if pad.buttons.remove(button) {
                    self.pending.pad_released.insert((id, *button));
                }
            }
            GamepadEventKind::AxisChanged(axis, value) => {
                let value = if value.is_nan() {
                    0.0
                } else {
                    value.clamp(-1.0, 1.0)
                };
                let pad = self.pad_entry(id);
                let old = std::mem::replace(&mut pad.axes[axis.index()], value);
                for positive in [false, true] {
                    match (holds(old, positive), holds(value, positive)) {
                        (false, true) => {
                            self.pending.stick_pressed.insert((id, (*axis, positive)));
                        }
                        (true, false) => {
                            self.pending.stick_released.insert((id, (*axis, positive)));
                        }
                        _ => {}
                    }
                }
            }
            // Later kinds of gamepad events change nothing here.
            _ => {}
        }
    }

    /// The pad `id`, added unnamed if its connection was missed.
    fn pad_entry(&mut self, id: GamepadId) -> &mut Gamepad {
        self.pads.entry(id).or_insert_with(|| Gamepad {
            id,
            name: String::new(),
            buttons: HashSet::new(),
            axes: [0.0; 6],
        })
    }

    /// Updates the state with an input event received at `now`.
    pub fn handle_event(&mut self, event: &Event, now: Instant) {
        match event {
            Event::Key(key) => {
                self.modifiers = key.modifiers;
                let code = normalize(key.code);
                match key.kind {
                    KeyEventKind::Press | KeyEventKind::Repeat => {
                        if self.down.insert(code, now).is_none() {
                            self.pending.pressed.insert(code);
                        }
                    }
                    KeyEventKind::Release => {
                        if self.down.remove(&code).is_some() {
                            self.pending.released.insert(code);
                        }
                    }
                }
            }
            Event::Mouse(mouse) => {
                self.modifiers = mouse.modifiers;
                self.mouse_position = Some(Position::new(mouse.column, mouse.row));
                match mouse.kind {
                    MouseEventKind::Down(button) => {
                        let index = button_index(button);
                        self.mouse_down[index] = true;
                        self.pending.mouse_pressed[index] = true;
                    }
                    MouseEventKind::Up(button) => {
                        let index = button_index(button);
                        self.mouse_down[index] = false;
                        self.pending.mouse_released[index] = true;
                    }
                    MouseEventKind::ScrollDown => self.pending.scroll.1 += 1,
                    MouseEventKind::ScrollUp => self.pending.scroll.1 -= 1,
                    MouseEventKind::ScrollRight => self.pending.scroll.0 += 1,
                    MouseEventKind::ScrollLeft => self.pending.scroll.0 -= 1,
                    MouseEventKind::Drag(_) | MouseEventKind::Moved => {}
                }
            }
            Event::FocusLost => self.release_all(),
            Event::Gamepad(pad) => self.handle_gamepad(pad.id, &pad.kind),
            // Resizes, focus gains, pastes, closing and later kinds of events change no
            // held keys or buttons.
            _ => {}
        }
    }

    /// Releases every key and mouse button (used when the window loses focus, because
    /// releases that happen in another window are never reported).
    pub fn release_all(&mut self) {
        for (key, _) in self.down.drain() {
            self.pending.released.insert(key);
        }
        for index in 0..self.mouse_down.len() {
            if self.mouse_down[index] {
                self.mouse_down[index] = false;
                self.pending.mouse_released[index] = true;
            }
        }
        self.modifiers = KeyModifiers::NONE;
    }

    /// Starts an update at `now`: applies release timeouts and makes the collected changes
    /// visible.
    pub(crate) fn begin_update(&mut self, now: Instant) {
        if !self.key_release_events {
            let timeout = self.hold_timeout;
            let pending = &mut self.pending;
            self.down.retain(|key, last_seen| {
                let held = now.saturating_duration_since(*last_seen) < timeout;
                if !held {
                    pending.released.insert(*key);
                }
                held
            });
        }
        std::mem::swap(&mut self.current, &mut self.pending);
        self.pending.clear();
    }

    /// Ends an update: the changes have been seen.
    pub(crate) fn end_update(&mut self) {
        self.current.clear();
    }

    pub(crate) fn set_key_release_events(&mut self, enabled: bool) {
        self.key_release_events = enabled;
    }

    pub(crate) fn set_hold_timeout(&mut self, timeout: Duration) {
        self.hold_timeout = timeout;
    }
}

/// Makes keys comparable regardless of Shift: letters become lowercase and `BackTab`
/// (Shift+Tab) becomes `Tab`.
fn normalize(key: KeyCode) -> KeyCode {
    match key {
        KeyCode::Char(c) if c.is_uppercase() => {
            let mut lower = c.to_lowercase();
            match (lower.next(), lower.next()) {
                (Some(single), None) => KeyCode::Char(single),
                _ => key,
            }
        }
        KeyCode::BackTab => KeyCode::Tab,
        other => other,
    }
}

fn button_index(button: MouseButton) -> usize {
    match button {
        MouseButton::Left => 0,
        MouseButton::Right => 1,
        MouseButton::Middle => 2,
    }
}

#[cfg(test)]
mod tests {
    use vnce_term::KeyEvent;
    use vnce_term::input::MouseEvent;

    use super::*;

    const MS: Duration = Duration::from_millis(1);

    fn key(code: KeyCode, kind: KeyEventKind) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind,
        })
    }

    fn mouse(kind: MouseEventKind) -> Event {
        Event::Mouse(MouseEvent {
            kind,
            column: 4,
            row: 2,
            modifiers: KeyModifiers::NONE,
        })
    }

    #[test]
    fn press_and_release_with_release_events() {
        let t = Instant::now();
        let mut input = InputState::new(true, MS * 500);
        let a = KeyCode::Char('a');

        input.handle_event(&key(a, KeyEventKind::Press), t);
        input.begin_update(t);
        assert!(input.is_down(a) && input.was_pressed(a) && !input.was_released(a));
        input.end_update();

        // Repeats do not count as new presses, and no timeout applies.
        input.handle_event(&key(a, KeyEventKind::Repeat), t + MS * 10);
        input.begin_update(t + MS * 5000);
        assert!(input.is_down(a) && !input.was_pressed(a));
        input.end_update();

        input.handle_event(&key(a, KeyEventKind::Release), t + MS * 6000);
        input.begin_update(t + MS * 6000);
        assert!(!input.is_down(a) && input.was_released(a));
        input.end_update();
        input.begin_update(t + MS * 6020);
        assert!(!input.was_released(a));
    }

    #[test]
    fn changes_wait_for_the_next_update() {
        let t = Instant::now();
        let mut input = InputState::new(true, MS * 500);
        let a = KeyCode::Char('a');
        // A tap shorter than one update is still seen.
        input.handle_event(&key(a, KeyEventKind::Press), t);
        input.handle_event(&key(a, KeyEventKind::Release), t + MS);
        input.begin_update(t + MS * 16);
        assert!(input.was_pressed(a) && input.was_released(a) && !input.is_down(a));
    }

    #[test]
    fn timeout_releases_keys_without_release_events() {
        let t = Instant::now();
        let mut input = InputState::new(false, MS * 500);
        let up = KeyCode::Up;

        input.handle_event(&key(up, KeyEventKind::Press), t);
        input.begin_update(t + MS * 100);
        assert!(input.is_down(up) && input.was_pressed(up));
        input.end_update();

        // Repeats keep the key held.
        input.handle_event(&key(up, KeyEventKind::Press), t + MS * 450);
        input.begin_update(t + MS * 900);
        assert!(input.is_down(up) && !input.was_pressed(up));
        input.end_update();

        input.begin_update(t + MS * 950);
        assert!(!input.is_down(up) && input.was_released(up));
    }

    #[test]
    fn letters_are_case_insensitive() {
        let t = Instant::now();
        let mut input = InputState::new(true, MS * 500);
        input.handle_event(&key(KeyCode::Char('A'), KeyEventKind::Press), t);
        input.handle_event(&key(KeyCode::Char('a'), KeyEventKind::Release), t);
        input.begin_update(t);
        assert!(!input.is_down(KeyCode::Char('A')));
        assert!(input.was_released(KeyCode::Char('A')));

        input.handle_event(&key(KeyCode::BackTab, KeyEventKind::Press), t);
        assert!(input.is_down(KeyCode::Tab));
        assert!(input.any_down(&[KeyCode::Esc, KeyCode::BackTab]));
        assert_eq!(input.keys_down().collect::<Vec<_>>(), vec![KeyCode::Tab]);
    }

    #[test]
    fn focus_loss_releases_everything() {
        let t = Instant::now();
        let mut input = InputState::new(true, MS * 500);
        input.handle_event(&key(KeyCode::Left, KeyEventKind::Press), t);
        input.handle_event(&mouse(MouseEventKind::Down(MouseButton::Left)), t);
        input.handle_event(&Event::FocusLost, t);
        input.begin_update(t);
        assert!(!input.is_down(KeyCode::Left));
        assert!(input.was_released(KeyCode::Left));
        assert!(!input.is_mouse_down(MouseButton::Left));
        assert!(input.was_mouse_released(MouseButton::Left));
    }

    #[test]
    fn mouse_state() {
        let t = Instant::now();
        let mut input = InputState::new(true, MS * 500);
        input.handle_event(&mouse(MouseEventKind::Down(MouseButton::Right)), t);
        input.handle_event(&mouse(MouseEventKind::ScrollDown), t);
        input.handle_event(&mouse(MouseEventKind::ScrollDown), t);
        input.handle_event(&mouse(MouseEventKind::ScrollLeft), t);
        input.begin_update(t);
        assert_eq!(input.mouse_position(), Some(Position::new(4, 2)));
        assert!(input.is_mouse_down(MouseButton::Right));
        assert!(input.was_mouse_pressed(MouseButton::Right));
        assert!(!input.was_mouse_pressed(MouseButton::Left));
        assert_eq!(input.scroll(), (-1, 2));
        input.end_update();
        assert_eq!(input.scroll(), (0, 0));

        input.handle_event(&mouse(MouseEventKind::Up(MouseButton::Right)), t);
        input.begin_update(t);
        assert!(input.was_mouse_released(MouseButton::Right));
        assert!(!input.is_mouse_down(MouseButton::Right));
    }

    fn pad(id: u32, kind: GamepadEventKind) -> Event {
        Event::Gamepad(vnce_term::GamepadEvent::new(GamepadId(id), kind))
    }

    #[test]
    fn gamepad_buttons_and_pads() {
        let t = Instant::now();
        let mut input = InputState::new(true, MS * 500);
        input.handle_event(&pad(1, GamepadEventKind::Connected("Pad".into())), t);
        input.handle_event(&pad(2, GamepadEventKind::Connected("Other".into())), t);
        assert_eq!(input.gamepads().count(), 2);
        assert_eq!(input.gamepad(GamepadId(1)).map(Gamepad::name), Some("Pad"));
        let south = GamepadButton::South;
        input.handle_event(&pad(2, GamepadEventKind::ButtonPressed(south)), t);
        input.begin_update(t);
        assert!(input.is_button_down(south) && input.was_button_pressed(south));
        assert!(input.was_pad_button_pressed(GamepadId(2), south));
        assert!(!input.was_pad_button_pressed(GamepadId(1), south));
        input.end_update();
        input.begin_update(t);
        assert!(input.is_button_down(south) && !input.was_button_pressed(south));
        input.end_update();
        // Unplugging releases what it held.
        input.handle_event(&pad(2, GamepadEventKind::Disconnected), t);
        input.begin_update(t);
        assert!(!input.is_button_down(south) && input.was_button_released(south));
        assert_eq!(input.gamepads().count(), 1);
        input.end_update();
        // Focus does not release pads (they keep working in the background).
        input.handle_event(&pad(1, GamepadEventKind::ButtonPressed(south)), t);
        input.handle_event(&Event::FocusLost, t);
        input.begin_update(t);
        assert!(input.is_button_down(south));
    }

    #[test]
    fn sticks_dead_zone_and_directions() {
        let t = Instant::now();
        let mut input = InputState::new(true, MS * 500);
        let x = GamepadAxis::LeftX;
        input.handle_event(&pad(0, GamepadEventKind::AxisChanged(x, 0.1)), t);
        input.begin_update(t);
        assert!(input.axis(x).abs() < f32::EPSILON, "inside the dead zone");
        assert!(!input.is_stick_down(x, true));
        input.end_update();
        input.handle_event(&pad(0, GamepadEventKind::AxisChanged(x, -0.8)), t);
        input.begin_update(t);
        assert!(input.axis(x) < -0.7);
        assert!(input.is_stick_down(x, false) && input.was_stick_pressed(x, false));
        assert!(!input.is_stick_down(x, true));
        let pad0 = input.gamepad(GamepadId(0)).unwrap();
        assert!((pad0.axis_raw(x) + 0.8).abs() < f32::EPSILON);
        input.end_update();
        input.handle_event(&pad(0, GamepadEventKind::AxisChanged(x, 1.0)), t);
        input.begin_update(t);
        assert!(
            (input.axis(x) - 1.0).abs() < f32::EPSILON,
            "full range after the zone"
        );
        assert!(input.was_stick_released(x, false) && input.was_stick_pressed(x, true));
        input.end_update();
        input.handle_event(&pad(0, GamepadEventKind::AxisChanged(x, f32::NAN)), t);
        input.begin_update(t);
        assert!(input.axis(x).abs() < f32::EPSILON && input.was_stick_released(x, true));
    }
}
