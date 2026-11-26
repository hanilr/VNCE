//! Gamepad events: buttons and axes in a standard layout.
//!
//! Terminals cannot read gamepads; a native window (`vnce-window` with its `gamepad`
//! feature) sends these events, and tests can send them with a scripted driver. Buttons
//! are named by position (as on most controllers): [`GamepadButton::South`] is A on an
//! Xbox pad and Cross on a Sony pad.

use std::fmt;
use std::hash::{Hash, Hasher};

/// Which gamepad an event comes from. Ids are given out as pads connect and stay the same
/// while a pad is connected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct GamepadId(pub u32);

/// A gamepad button in the standard layout.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum GamepadButton {
    /// The bottom face button (A on Xbox, Cross on Sony pads).
    South,
    /// The right face button (B, Circle).
    East,
    /// The left face button (X, Square).
    West,
    /// The top face button (Y, Triangle).
    North,
    /// The left shoulder button (LB, L1).
    LeftShoulder,
    /// The right shoulder button (RB, R1).
    RightShoulder,
    /// The left trigger pressed as a button (LT, L2); its travel is
    /// [`GamepadAxis::LeftTrigger`].
    LeftTrigger,
    /// The right trigger pressed as a button (RT, R2).
    RightTrigger,
    /// Pressing the left stick (L3).
    LeftStick,
    /// Pressing the right stick (R3).
    RightStick,
    /// The d-pad up.
    DPadUp,
    /// The d-pad down.
    DPadDown,
    /// The d-pad left.
    DPadLeft,
    /// The d-pad right.
    DPadRight,
    /// Start (Menu, Options).
    Start,
    /// Select (View, Share).
    Select,
    /// The logo button (Guide, PS).
    Mode,
}

impl GamepadButton {
    /// Every button, in the order above.
    pub const ALL: [Self; 17] = [
        Self::South,
        Self::East,
        Self::West,
        Self::North,
        Self::LeftShoulder,
        Self::RightShoulder,
        Self::LeftTrigger,
        Self::RightTrigger,
        Self::LeftStick,
        Self::RightStick,
        Self::DPadUp,
        Self::DPadDown,
        Self::DPadLeft,
        Self::DPadRight,
        Self::Start,
        Self::Select,
        Self::Mode,
    ];

    /// The name used by `Display` and `FromStr` (`South`, `DPadUp`, ...).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::South => "South",
            Self::East => "East",
            Self::West => "West",
            Self::North => "North",
            Self::LeftShoulder => "LeftShoulder",
            Self::RightShoulder => "RightShoulder",
            Self::LeftTrigger => "LeftTrigger",
            Self::RightTrigger => "RightTrigger",
            Self::LeftStick => "LeftStick",
            Self::RightStick => "RightStick",
            Self::DPadUp => "DPadUp",
            Self::DPadDown => "DPadDown",
            Self::DPadLeft => "DPadLeft",
            Self::DPadRight => "DPadRight",
            Self::Start => "Start",
            Self::Select => "Select",
            Self::Mode => "Mode",
        }
    }
}

impl fmt::Display for GamepadButton {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl std::str::FromStr for GamepadButton {
    type Err = ParseGamepadError;

    /// Parses a name written by `Display`, ignoring case.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|b| b.name().eq_ignore_ascii_case(s.trim()))
            .ok_or_else(|| ParseGamepadError(s.to_owned()))
    }
}

/// A gamepad axis. Sticks go from `-1.0` to `1.0` (x: right is positive; y: up is
/// positive), triggers from `0.0` (released) to `1.0` (pulled all the way).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum GamepadAxis {
    /// The left stick, left to right.
    LeftX,
    /// The left stick, down to up.
    LeftY,
    /// The right stick, left to right.
    RightX,
    /// The right stick, down to up.
    RightY,
    /// The left trigger's travel.
    LeftTrigger,
    /// The right trigger's travel.
    RightTrigger,
}

impl GamepadAxis {
    /// Every axis, in the order above.
    pub const ALL: [Self; 6] = [
        Self::LeftX,
        Self::LeftY,
        Self::RightX,
        Self::RightY,
        Self::LeftTrigger,
        Self::RightTrigger,
    ];

    /// The name used by `Display` and `FromStr` (`LeftX`, `RightTrigger`, ...).
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::LeftX => "LeftX",
            Self::LeftY => "LeftY",
            Self::RightX => "RightX",
            Self::RightY => "RightY",
            Self::LeftTrigger => "LeftTriggerAxis",
            Self::RightTrigger => "RightTriggerAxis",
        }
    }

    /// The position of this axis in [`GamepadAxis::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        match self {
            Self::LeftX => 0,
            Self::LeftY => 1,
            Self::RightX => 2,
            Self::RightY => 3,
            Self::LeftTrigger => 4,
            Self::RightTrigger => 5,
        }
    }
}

impl fmt::Display for GamepadAxis {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

impl std::str::FromStr for GamepadAxis {
    type Err = ParseGamepadError;

    /// Parses a name written by `Display`, ignoring case.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|a| a.name().eq_ignore_ascii_case(s.trim()))
            .ok_or_else(|| ParseGamepadError(s.to_owned()))
    }
}

/// The error returned when a gamepad button or axis name cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseGamepadError(String);

impl fmt::Display for ParseGamepadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown gamepad button or axis {:?}", self.0)
    }
}

impl std::error::Error for ParseGamepadError {}

/// What happened on a gamepad.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum GamepadEventKind {
    /// The pad was connected (or was already connected when the game started), with its
    /// name.
    Connected(String),
    /// The pad was disconnected; its buttons count as released.
    Disconnected,
    /// A button was pressed.
    ButtonPressed(GamepadButton),
    /// A button was released.
    ButtonReleased(GamepadButton),
    /// An axis moved to a new value (see [`GamepadAxis`] for the ranges).
    AxisChanged(GamepadAxis, f32),
}

// Axis values are compared and hashed by their bits, so events can be compared in tests
// and kept in sets (`NaN` never comes from a pad).
impl PartialEq for GamepadEventKind {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Connected(a), Self::Connected(b)) => a == b,
            (Self::Disconnected, Self::Disconnected) => true,
            (Self::ButtonPressed(a), Self::ButtonPressed(b))
            | (Self::ButtonReleased(a), Self::ButtonReleased(b)) => a == b,
            (Self::AxisChanged(a, x), Self::AxisChanged(b, y)) => {
                a == b && x.to_bits() == y.to_bits()
            }
            _ => false,
        }
    }
}

impl Eq for GamepadEventKind {}

impl Hash for GamepadEventKind {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::mem::discriminant(self).hash(state);
        match self {
            Self::Connected(name) => name.hash(state),
            Self::Disconnected => {}
            Self::ButtonPressed(button) | Self::ButtonReleased(button) => button.hash(state),
            Self::AxisChanged(axis, value) => {
                axis.hash(state);
                value.to_bits().hash(state);
            }
        }
    }
}

/// A gamepad event: which pad and what happened.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GamepadEvent {
    /// The pad.
    pub id: GamepadId,
    /// What happened.
    pub kind: GamepadEventKind,
}

impl GamepadEvent {
    /// An event of pad `id`.
    #[must_use]
    pub fn new(id: GamepadId, kind: GamepadEventKind) -> Self {
        Self { id, kind }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn names_round_trip() {
        for button in GamepadButton::ALL {
            assert_eq!(button.to_string().parse(), Ok(button));
        }
        for axis in GamepadAxis::ALL {
            assert_eq!(axis.to_string().parse(), Ok(axis));
            assert_eq!(GamepadAxis::ALL[axis.index()], axis);
        }
        assert_eq!("dpadup".parse(), Ok(GamepadButton::DPadUp));
        assert!("Turbo".parse::<GamepadButton>().is_err());
        assert!("LeftZ".parse::<GamepadAxis>().is_err());
    }

    #[test]
    fn events_compare_and_hash() {
        let pad = GamepadId(1);
        let a = GamepadEvent::new(pad, GamepadEventKind::AxisChanged(GamepadAxis::LeftX, 0.5));
        let b = GamepadEvent::new(pad, GamepadEventKind::AxisChanged(GamepadAxis::LeftX, 0.5));
        let c = GamepadEvent::new(pad, GamepadEventKind::AxisChanged(GamepadAxis::LeftX, 0.6));
        assert_eq!(a, b);
        assert_ne!(a, c);
        let set: HashSet<_> = [a, b, c].into_iter().collect();
        assert_eq!(set.len(), 2);
        assert_ne!(
            GamepadEventKind::ButtonPressed(GamepadButton::South),
            GamepadEventKind::ButtonReleased(GamepadButton::South)
        );
    }
}
