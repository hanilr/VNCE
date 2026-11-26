//! Key bindings: [`ActionMap`] maps keys, mouse buttons and gamepad buttons and sticks to
//! game actions.

use std::fmt;
use std::str::FromStr;

use vnce_runtime::InputState;
use vnce_term::input::MouseButton;
use vnce_term::{GamepadAxis, GamepadButton, KeyCode};

/// A key, mouse button, gamepad button or stick direction bound to an action.
///
/// New kinds of bindings may be added in later versions, so a `match` needs a `_` arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Binding {
    /// A key. Letters match both cases.
    Key(KeyCode),
    /// A mouse button.
    Mouse(MouseButton),
    /// A gamepad button (on any pad).
    Button(GamepadButton),
    /// A stick or trigger pushed past
    /// [`STICK_PRESS`](vnce_runtime::STICK_PRESS) in one direction (on any pad).
    Stick {
        /// The axis.
        axis: GamepadAxis,
        /// `true` for right / up / pulled, `false` for left / down.
        positive: bool,
    },
}

impl Binding {
    /// A stick direction: `Binding::stick(GamepadAxis::LeftX, false)` is the left stick
    /// pushed left.
    #[must_use]
    pub fn stick(axis: GamepadAxis, positive: bool) -> Self {
        Self::Stick { axis, positive }
    }

    /// Returns `true` for gamepad buttons and sticks.
    #[must_use]
    pub fn is_gamepad(&self) -> bool {
        matches!(self, Self::Button(_) | Self::Stick { .. })
    }
}

impl From<GamepadButton> for Binding {
    fn from(button: GamepadButton) -> Self {
        Self::Button(button)
    }
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
    /// `mouse:middle`, a gamepad button as `pad:South` and a stick direction as
    /// `pad:LeftX-` or `pad:LeftY+`. [`FromStr`] reads it back, for example from a save
    /// file.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Key(key) => write!(f, "{key}"),
            Self::Mouse(MouseButton::Left) => f.write_str("mouse:left"),
            Self::Mouse(MouseButton::Right) => f.write_str("mouse:right"),
            Self::Mouse(MouseButton::Middle) => f.write_str("mouse:middle"),
            Self::Button(button) => write!(f, "pad:{button}"),
            Self::Stick { axis, positive } => {
                write!(f, "pad:{axis}{}", if *positive { '+' } else { '-' })
            }
        }
    }
}

/// The error returned when a binding name cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseBindingError(String);

impl fmt::Display for ParseBindingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown key, mouse button or gamepad input {:?}", self.0)
    }
}

impl std::error::Error for ParseBindingError {}

impl FromStr for Binding {
    type Err = ParseBindingError;

    /// Parses a name written by `Display` (`w`, `Space`, `Esc`, `mouse:left`,
    /// `pad:South`, `pad:LeftX-`, ...).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let error = || ParseBindingError(s.to_owned());
        let lower = s.to_ascii_lowercase();
        if lower.starts_with("pad:") {
            let name = &s.trim()[4..];
            if let Some(axis) = name.strip_suffix('+') {
                return axis
                    .parse()
                    .map(|a| Self::stick(a, true))
                    .map_err(|_| error());
            }
            if let Some(axis) = name.strip_suffix('-') {
                return axis
                    .parse()
                    .map(|a| Self::stick(a, false))
                    .map_err(|_| error());
            }
            return name.parse().map(Self::Button).map_err(|_| error());
        }
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

    /// Replaces the bindings of `action` on the same device as `binding` (keyboard and
    /// mouse, or gamepad) with `binding`, keeping those of the other device. A controls
    /// screen that rebinds a key keeps the action's gamepad button, and the other way
    /// round.
    pub fn rebind_device(&mut self, action: A, binding: impl Into<Binding>) {
        let binding = binding.into();
        let pad = binding.is_gamepad();
        self.bindings
            .retain(|(a, b)| *a != action || b.is_gamepad() != pad);
        self.add(action, binding);
    }

    /// Replaces the bindings of `action` with `bindings` for each device that `bindings`
    /// has (keyboard and mouse, gamepad), keeping the others. For loading saved controls:
    /// a save file from before gamepads were bound keeps the default gamepad bindings.
    pub fn replace_bindings(&mut self, action: A, bindings: impl IntoIterator<Item = Binding>) {
        let bindings: Vec<Binding> = bindings.into_iter().collect();
        let keys = bindings.iter().any(|b| !b.is_gamepad());
        let pads = bindings.iter().any(Binding::is_gamepad);
        self.bindings
            .retain(|(a, b)| *a != action || if b.is_gamepad() { !pads } else { !keys });
        for binding in bindings {
            self.add(action, binding);
        }
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
            Binding::Button(button) => input.is_button_down(button),
            Binding::Stick { axis, positive } => input.is_stick_down(axis, positive),
        })
    }

    /// Returns `true` if any binding of `action` was pressed since the previous update.
    #[must_use]
    pub fn was_pressed(&self, action: A, input: &InputState) -> bool {
        self.bindings(action).any(|binding| match binding {
            Binding::Key(key) => input.was_pressed(key),
            Binding::Mouse(button) => input.was_mouse_pressed(button),
            Binding::Button(button) => input.was_button_pressed(button),
            Binding::Stick { axis, positive } => input.was_stick_pressed(axis, positive),
        })
    }

    /// Returns `true` if any binding of `action` was released since the previous update.
    #[must_use]
    pub fn was_released(&self, action: A, input: &InputState) -> bool {
        self.bindings(action).any(|binding| match binding {
            Binding::Key(key) => input.was_released(key),
            Binding::Mouse(button) => input.was_mouse_released(button),
            Binding::Button(button) => input.was_button_released(button),
            Binding::Stick { axis, positive } => input.was_stick_released(axis, positive),
        })
    }

    /// How strongly `action` is held, from `0.0` to `1.0`: `1.0` for a held key or button,
    /// the stick's push (after the dead zone) for stick bindings, the strongest of them.
    #[must_use]
    pub fn value(&self, action: A, input: &InputState) -> f32 {
        self.bindings(action)
            .map(|binding| match binding {
                Binding::Key(key) => f32::from(u8::from(input.is_down(key))),
                Binding::Mouse(button) => f32::from(u8::from(input.is_mouse_down(button))),
                Binding::Button(button) => f32::from(u8::from(input.is_button_down(button))),
                Binding::Stick { axis, positive } => {
                    let v = input.axis(axis);
                    if positive { v.max(0.0) } else { (-v).max(0.0) }
                }
            })
            .fold(0.0, f32::max)
    }

    /// From `-1.0` (only `negative` held) to `1.0` (only `positive` held); `0.0` when
    /// neither or both are held. Keys give whole steps, sticks everything in between. For
    /// movement: `x += speed * axis(Left, Right)`.
    #[must_use]
    pub fn axis(&self, negative: A, positive: A, input: &InputState) -> f32 {
        (self.value(positive, input) - self.value(negative, input)).clamp(-1.0, 1.0)
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
        for binding in [
            Binding::from(GamepadButton::South),
            Binding::from(GamepadButton::DPadLeft),
            Binding::stick(GamepadAxis::LeftX, false),
            Binding::stick(GamepadAxis::RightTrigger, true),
        ] {
            let name = binding.to_string();
            assert_eq!(name.parse::<Binding>(), Ok(binding), "{name}");
            assert!(binding.is_gamepad());
        }
        assert_eq!(
            Binding::stick(GamepadAxis::LeftY, true).to_string(),
            "pad:LeftY+"
        );
        assert_eq!(
            "PAD:south".parse(),
            Ok(Binding::Button(GamepadButton::South))
        );
        assert!("pad:LeftZ+".parse::<Binding>().is_err());
        assert!("pad:Turbo".parse::<Binding>().is_err());
        assert!(!Binding::from('w').is_gamepad());
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
    fn gamepad_buttons_and_sticks() {
        use vnce_term::{GamepadEvent, GamepadEventKind, GamepadId};

        let controls = controls()
            .bind(Action::Fire, GamepadButton::South)
            .bind(Action::Left, Binding::stick(GamepadAxis::LeftX, false))
            .bind(Action::Right, Binding::stick(GamepadAxis::LeftX, true));
        let mut input = InputState::new(true, Duration::from_millis(500));
        let pad = |kind| Event::Gamepad(GamepadEvent::new(GamepadId(0), kind));
        let now = Instant::now();
        input.handle_event(
            &pad(GamepadEventKind::ButtonPressed(GamepadButton::South)),
            now,
        );
        assert!(controls.is_down(Action::Fire, &input));
        // Half a push to the left: a smooth axis, not a whole step.
        input.handle_event(
            &pad(GamepadEventKind::AxisChanged(GamepadAxis::LeftX, -0.575)),
            now,
        );
        let x = controls.axis(Action::Left, Action::Right, &input);
        assert!((x + 0.5).abs() < 1e-4, "{x}");
        assert!(controls.is_down(Action::Left, &input));
        // A key still gives a whole step and wins over the stick.
        press(&mut input, KeyCode::Left);
        assert!((controls.axis(Action::Left, Action::Right, &input) + 1.0).abs() < 1e-6);
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
        // Keys and gamepad buttons are replaced separately.
        controls.add(Action::Left, GamepadButton::DPadLeft);
        controls.rebind_device(Action::Left, 'h');
        assert_eq!(
            controls.bindings(Action::Left).collect::<Vec<_>>(),
            [Binding::Button(GamepadButton::DPadLeft), Binding::from('h')]
        );
        controls.rebind_device(Action::Left, GamepadButton::West);
        assert_eq!(
            controls.bindings(Action::Left).collect::<Vec<_>>(),
            [Binding::from('h'), Binding::Button(GamepadButton::West)]
        );
        controls.replace_bindings(Action::Left, [Binding::from('k')]);
        assert_eq!(
            controls.bindings(Action::Left).collect::<Vec<_>>(),
            [Binding::Button(GamepadButton::West), Binding::from('k')]
        );
        controls.unbind(Action::Fire);
        assert_eq!(controls.bindings(Action::Fire).count(), 0);
        assert_eq!(controls.bindings(Action::Right).count(), 1);
    }
}
