//! Key bindings: [`ActionMap`] maps keys and mouse buttons to game actions.

use std::fmt;
use std::str::FromStr;

use vnce_runtime::InputState;
use vnce_term::KeyCode;
use vnce_term::input::MouseButton;

/// A key or mouse button bound to an action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Binding {
    /// A key. Letters match both cases.
    Key(KeyCode),
    /// A mouse button.
    Mouse(MouseButton),
}

impl From<KeyCode> for Binding {
    fn from(key: KeyCode) -> Self {
        Self::Key(key)
    }
}

impl From<char> for Binding {
    fn from(c: char) -> Self {
        Self::Key(KeyCode::Char(c))
    }
}

impl From<MouseButton> for Binding {
    fn from(button: MouseButton) -> Self {
        Self::Mouse(button)
    }
}

impl fmt::Display for Binding {
    /// The key name (see [`KeyCode`]'s `Display`) or `mouse:left`, `mouse:right`,
    /// `mouse:middle`. [`FromStr`] reads it back, for example from a save file.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Key(key) => write!(f, "{key}"),
            Self::Mouse(MouseButton::Left) => f.write_str("mouse:left"),
            Self::Mouse(MouseButton::Right) => f.write_str("mouse:right"),
            Self::Mouse(MouseButton::Middle) => f.write_str("mouse:middle"),
        }
    }
}

/// The error returned when a binding name cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseBindingError(String);

impl fmt::Display for ParseBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown key or mouse button {:?}", self.0)
    }
}

impl std::error::Error for ParseBindingError {}

impl FromStr for Binding {
    type Err = ParseBindingError;

    /// Parses a name written by `Display` (`w`, `Space`, `Esc`, `mouse:left`, ...).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let error = || ParseBindingError(s.to_owned());
        let lower = s.to_ascii_lowercase();
        if let Some(button) = lower.strip_prefix("mouse:") {
            return match button {
                "left" => Ok(Self::Mouse(MouseButton::Left)),
                "right" => Ok(Self::Mouse(MouseButton::Right)),
                "middle" => Ok(Self::Mouse(MouseButton::Middle)),
                _ => Err(error()),
            };
        }
        s.parse().map(Self::Key).map_err(|_| error())
    }
}

/// Maps game actions (your own type, usually an enum) to keys and mouse buttons.
///
/// The game asks about actions instead of keys, so controls can be changed (for example
/// in a settings screen) without touching the game logic.
///
/// ```
/// use vnce_game::ActionMap;
/// use vnce_term::KeyCode;
///
/// #[derive(Clone, Copy, PartialEq, Eq, Debug)]
/// enum Action {
///     Left,
///     Right,
///     Jump,
/// }
///
/// let controls = ActionMap::new()
///     .bind(Action::Left, KeyCode::Left)
///     .bind(Action::Left, 'a')
///     .bind(Action::Right, KeyCode::Right)
///     .bind(Action::Right, 'd')
///     .bind(Action::Jump, ' ');
/// assert_eq!(controls.bindings(Action::Left).count(), 2);
/// // In `Game::update`:
/// // let dx = controls.axis(Action::Left, Action::Right, ctx.input());
/// // if controls.was_pressed(Action::Jump, ctx.input()) { ... }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionMap<A> {
    bindings: Vec<(A, Binding)>,
}

impl<A> Default for ActionMap<A> {
    fn default() -> Self {
        Self {
            bindings: Vec::new(),
        }
    }
}

impl<A: Copy + PartialEq> ActionMap<A> {
    /// An empty map.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a binding (builder style). An action can have any number of bindings.
    #[must_use]
    pub fn bind(mut self, action: A, binding: impl Into<Binding>) -> Self {
        self.add(action, binding);
        self
    }

    /// Adds a binding. Binding the same key to the same action twice has no effect.
    pub fn add(&mut self, action: A, binding: impl Into<Binding>) {
        let entry = (action, binding.into());
        if !self.bindings.contains(&entry) {
            self.bindings.push(entry);
        }
    }

    /// Removes every binding of `action`.
    pub fn unbind(&mut self, action: A) {
        self.bindings.retain(|(a, _)| *a != action);
    }

    /// Replaces every binding of `action` with `binding` (for rebinding in a settings
    /// screen).
    pub fn rebind(&mut self, action: A, binding: impl Into<Binding>) {
        self.unbind(action);
        self.add(action, binding);
    }

    /// The bindings of `action`, in the order they were added.
    pub fn bindings(&self, action: A) -> impl Iterator<Item = Binding> + '_ {
        self.bindings
            .iter()
            .filter(move |(a, _)| *a == action)
            .map(|&(_, binding)| binding)
    }

    /// Returns `true` while any binding of `action` is held down.
    #[must_use]
    pub fn is_down(&self, action: A, input: &InputState) -> bool {
        self.bindings(action).any(|binding| match binding {
            Binding::Key(key) => input.is_down(key),
            Binding::Mouse(button) => input.is_mouse_down(button),
        })
    }

    /// Returns `true` if any binding of `action` was pressed since the previous update.
    #[must_use]
    pub fn was_pressed(&self, action: A, input: &InputState) -> bool {
        self.bindings(action).any(|binding| match binding {
            Binding::Key(key) => input.was_pressed(key),
            Binding::Mouse(button) => input.was_mouse_pressed(button),
        })
    }

    /// Returns `true` if any binding of `action` was released since the previous update.
    #[must_use]
    pub fn was_released(&self, action: A, input: &InputState) -> bool {
        self.bindings(action).any(|binding| match binding {
            Binding::Key(key) => input.was_released(key),
            Binding::Mouse(button) => input.was_mouse_released(button),
        })
    }

    /// `-1.0` while only `negative` is held, `1.0` while only `positive` is held, `0.0`
    /// otherwise. For movement: `x += speed * axis(Left, Right)`.
    #[must_use]
    pub fn axis(&self, negative: A, positive: A, input: &InputState) -> f32 {
        match (self.is_down(negative, input), self.is_down(positive, input)) {
            (true, false) => -1.0,
            (false, true) => 1.0,
            _ => 0.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use vnce_term::input::{MouseEvent, MouseEventKind};
    use vnce_term::{Event, KeyEvent, KeyEventKind, KeyModifiers};

    use super::*;

    #[test]
    fn bindings_round_trip() {
        for binding in [
            Binding::from('w'),
            Binding::from(' '),
            Binding::from(KeyCode::Esc),
            Binding::from(KeyCode::F(3)),
            Binding::from(MouseButton::Left),
            Binding::from(MouseButton::Right),
            Binding::from(MouseButton::Middle),
        ] {
            let name = binding.to_string();
            assert_eq!(name.parse::<Binding>(), Ok(binding), "{name}");
        }
        assert_eq!("Mouse:Left".parse(), Ok(Binding::Mouse(MouseButton::Left)));
        assert!("mouse:wheel".parse::<Binding>().is_err());
        assert!("nothing".parse::<Binding>().is_err());
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Action {
        Left,
        Right,
        Fire,
    }

    fn controls() -> ActionMap<Action> {
        ActionMap::new()
            .bind(Action::Left, KeyCode::Left)
            .bind(Action::Left, 'a')
            .bind(Action::Right, KeyCode::Right)
            .bind(Action::Fire, MouseButton::Left)
    }

    fn press(input: &mut InputState, code: KeyCode) {
        let event = Event::Key(KeyEvent {
            code,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        });
        input.handle_event(&event, Instant::now());
    }

    #[test]
    fn any_binding_activates_the_action() {
        let controls = controls();
        let mut input = InputState::new(true, Duration::from_millis(500));
        assert!(!controls.is_down(Action::Left, &input));
        // Letters match both cases.
        press(&mut input, KeyCode::Char('A'));
        assert!(controls.is_down(Action::Left, &input));
        assert!(!controls.is_down(Action::Right, &input));
        input.handle_event(
            &Event::Mouse(MouseEvent {
                kind: MouseEventKind::Down(MouseButton::Left),
                column: 0,
                row: 0,
                modifiers: KeyModifiers::NONE,
            }),
            Instant::now(),
        );
        assert!(controls.is_down(Action::Fire, &input));
    }

    #[test]
    fn axis_cancels_out() {
        let controls = controls();
        let mut input = InputState::new(true, Duration::from_millis(500));
        assert!((controls.axis(Action::Left, Action::Right, &input)).abs() < f32::EPSILON);
        press(&mut input, KeyCode::Right);
        assert!((controls.axis(Action::Left, Action::Right, &input) - 1.0).abs() < f32::EPSILON);
        press(&mut input, KeyCode::Left);
        assert!((controls.axis(Action::Left, Action::Right, &input)).abs() < f32::EPSILON);
    }

    #[test]
    fn rebinding_and_unbinding() {
        let mut controls = controls().bind(Action::Left, 'a');
        assert_eq!(controls.bindings(Action::Left).count(), 2);
        controls.rebind(Action::Left, 'j');
        assert_eq!(
            controls.bindings(Action::Left).collect::<Vec<_>>(),
            [Binding::Key(KeyCode::Char('j'))]
        );
        controls.unbind(Action::Fire);
        assert_eq!(controls.bindings(Action::Fire).count(), 0);
        assert_eq!(controls.bindings(Action::Right).count(), 1);
    }
}
