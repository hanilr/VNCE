//! Gamepads with `gilrs` (feature `gamepad`): their events become the engine's
//! [`Event::Gamepad`], and rumble requests become force feedback effects.
//!
//! Without the feature, or when the system offers no gamepad support, [`Gamepads`] is
//! empty and never sends events.

use std::collections::VecDeque;
use std::time::Duration;

use vnce_term::Event;

/// The window's gamepads.
pub(crate) struct Gamepads {
    #[cfg(feature = "gamepad")]
    inner: Option<real::Pads>,
}

impl Gamepads {
    /// No gamepads (until [`Gamepads::new`] replaces it).
    pub(crate) fn empty() -> Self {
        Self {
            #[cfg(feature = "gamepad")]
            inner: None,
        }
    }

    /// Starts reading gamepads; pads already connected are announced as connected.
    pub(crate) fn new(events: &mut VecDeque<Event>) -> Self {
        #[cfg(feature = "gamepad")]
        {
            Self {
                inner: real::Pads::new(events),
            }
        }
        #[cfg(not(feature = "gamepad"))]
        {
            let _ = events;
            Self {}
        }
    }

    /// Moves the gamepad events that arrived into `events`.
    #[cfg_attr(
        not(feature = "gamepad"),
        allow(clippy::unused_self, reason = "nothing to read without the feature")
    )]
    pub(crate) fn poll(&mut self, events: &mut VecDeque<Event>) {
        #[cfg(feature = "gamepad")]
        if let Some(pads) = &mut self.inner {
            pads.poll(events);
        }
        #[cfg(not(feature = "gamepad"))]
        let _ = events;
    }

    /// Rumbles every connected pad that can at `strength` (`0.0..=1.0`) for `duration`.
    #[allow(
        dead_code,
        reason = "the game asks for rumble through window control (step 4)"
    )]
    #[cfg_attr(
        not(feature = "gamepad"),
        allow(clippy::unused_self, reason = "nothing to rumble without the feature")
    )]
    pub(crate) fn rumble(&mut self, strength: f32, duration: Duration) {
        #[cfg(feature = "gamepad")]
        if let Some(pads) = &mut self.inner {
            pads.rumble(strength, duration);
        }
        #[cfg(not(feature = "gamepad"))]
        let _ = (strength, duration);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Starting, reading and rumbling never fail, with or without pads and gamepad
    /// support on the machine (CI machines have none).
    #[test]
    fn gamepads_start_without_pads() {
        let mut events = VecDeque::new();
        let mut pads = Gamepads::new(&mut events);
        pads.poll(&mut events);
        pads.rumble(0.5, Duration::from_millis(50));
        assert!(events.iter().all(|e| matches!(e, Event::Gamepad(_))));
        let mut none = Gamepads::empty();
        none.poll(&mut events);
    }
}

#[cfg(feature = "gamepad")]
mod real {
    use std::collections::VecDeque;
    use std::time::{Duration, Instant};

    use gilrs::ff::{BaseEffect, BaseEffectType, Effect, EffectBuilder, Repeat, Replay, Ticks};
    use gilrs::{Axis, Button, EventType, Gilrs};
    use vnce_term::{Event, GamepadAxis, GamepadButton, GamepadEvent, GamepadEventKind, GamepadId};

    pub(super) struct Pads {
        gilrs: Gilrs,
        /// Rumble effects still playing, with the time they end (dropping one stops it).
        effects: Vec<(Effect, Instant)>,
    }

    fn id(id: gilrs::GamepadId) -> GamepadId {
        GamepadId(u32::try_from(usize::from(id)).unwrap_or(u32::MAX))
    }

    /// gilrs' buttons in the engine's layout (gilrs calls the shoulders `LeftTrigger` and
    /// the triggers `LeftTrigger2`).
    fn button(button: Button) -> Option<GamepadButton> {
        Some(match button {
            Button::South => GamepadButton::South,
            Button::East => GamepadButton::East,
            Button::West => GamepadButton::West,
            Button::North => GamepadButton::North,
            Button::LeftTrigger => GamepadButton::LeftShoulder,
            Button::RightTrigger => GamepadButton::RightShoulder,
            Button::LeftTrigger2 => GamepadButton::LeftTrigger,
            Button::RightTrigger2 => GamepadButton::RightTrigger,
            Button::LeftThumb => GamepadButton::LeftStick,
            Button::RightThumb => GamepadButton::RightStick,
            Button::DPadUp => GamepadButton::DPadUp,
            Button::DPadDown => GamepadButton::DPadDown,
            Button::DPadLeft => GamepadButton::DPadLeft,
            Button::DPadRight => GamepadButton::DPadRight,
            Button::Start => GamepadButton::Start,
            Button::Select => GamepadButton::Select,
            Button::Mode => GamepadButton::Mode,
            _ => return None,
        })
    }

    fn axis(axis: Axis) -> Option<GamepadAxis> {
        Some(match axis {
            Axis::LeftStickX => GamepadAxis::LeftX,
            Axis::LeftStickY => GamepadAxis::LeftY,
            Axis::RightStickX => GamepadAxis::RightX,
            Axis::RightStickY => GamepadAxis::RightY,
            Axis::LeftZ => GamepadAxis::LeftTrigger,
            Axis::RightZ => GamepadAxis::RightTrigger,
            _ => return None,
        })
    }

    /// The engine's event for a gilrs event, if it has one.
    fn convert(event: &EventType, name: impl FnOnce() -> String) -> Option<GamepadEventKind> {
        Some(match *event {
            EventType::Connected => GamepadEventKind::Connected(name()),
            EventType::Disconnected => GamepadEventKind::Disconnected,
            EventType::ButtonPressed(b, _) => GamepadEventKind::ButtonPressed(button(b)?),
            EventType::ButtonReleased(b, _) => GamepadEventKind::ButtonReleased(button(b)?),
            // Analog triggers report their travel as a changing button.
            EventType::ButtonChanged(Button::LeftTrigger2, value, _) => {
                GamepadEventKind::AxisChanged(GamepadAxis::LeftTrigger, value)
            }
            EventType::ButtonChanged(Button::RightTrigger2, value, _) => {
                GamepadEventKind::AxisChanged(GamepadAxis::RightTrigger, value)
            }
            EventType::AxisChanged(a, value, _) => GamepadEventKind::AxisChanged(axis(a)?, value),
            _ => return None,
        })
    }

    impl Pads {
        pub(super) fn new(events: &mut VecDeque<Event>) -> Option<Self> {
            // Some systems have no gamepad support at all; the game then simply sees none.
            let gilrs = Gilrs::new().ok()?;
            for (pad, gamepad) in gilrs.gamepads() {
                events.push_back(Event::Gamepad(GamepadEvent::new(
                    id(pad),
                    GamepadEventKind::Connected(gamepad.name().to_owned()),
                )));
            }
            Some(Self {
                gilrs,
                effects: Vec::new(),
            })
        }

        pub(super) fn poll(&mut self, events: &mut VecDeque<Event>) {
            while let Some(event) = self.gilrs.next_event() {
                let gilrs = &self.gilrs;
                let name = || gilrs.gamepad(event.id).name().to_owned();
                if let Some(kind) = convert(&event.event, name) {
                    events.push_back(Event::Gamepad(GamepadEvent::new(id(event.id), kind)));
                }
            }
            let now = Instant::now();
            self.effects.retain(|(_, end)| *end > now);
        }

        #[allow(
            dead_code,
            reason = "the game asks for rumble through window control (step 4)"
        )]
        pub(super) fn rumble(&mut self, strength: f32, duration: Duration) {
            let pads: Vec<gilrs::GamepadId> = self
                .gilrs
                .gamepads()
                .filter(|(_, pad)| pad.is_ff_supported())
                .map(|(pad, _)| pad)
                .collect();
            if pads.is_empty() || strength <= 0.0 || duration.is_zero() {
                return;
            }
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                reason = "the strength is clamped to 0..=1"
            )]
            let magnitude = (strength.clamp(0.0, 1.0) * f32::from(u16::MAX)) as u16;
            let ms = u32::try_from(duration.as_millis()).unwrap_or(u32::MAX);
            let replay = Replay {
                play_for: Ticks::from_ms(ms),
                ..Replay::default()
            };
            let effect = EffectBuilder::new()
                .add_effect(BaseEffect {
                    kind: BaseEffectType::Strong { magnitude },
                    scheduling: replay,
                    ..BaseEffect::default()
                })
                .add_effect(BaseEffect {
                    kind: BaseEffectType::Weak { magnitude },
                    scheduling: replay,
                    ..BaseEffect::default()
                })
                .repeat(Repeat::For(Ticks::from_ms(ms)))
                .gamepads(&pads)
                .finish(&mut self.gilrs);
            // A pad that cannot play it simply does not rumble.
            if let Ok(effect) = effect {
                if effect.play().is_ok() {
                    self.effects.push((effect, Instant::now() + duration));
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn gilrs_events_map_to_the_engine_layout() {
            let name = || "Pad".to_owned();
            assert_eq!(
                convert(&EventType::Connected, name),
                Some(GamepadEventKind::Connected("Pad".to_owned()))
            );
            assert_eq!(
                button(Button::LeftTrigger),
                Some(GamepadButton::LeftShoulder)
            );
            assert_eq!(
                button(Button::LeftTrigger2),
                Some(GamepadButton::LeftTrigger)
            );
            assert_eq!(button(Button::C), None);
            assert_eq!(axis(Axis::RightZ), Some(GamepadAxis::RightTrigger));
            assert_eq!(axis(Axis::DPadX), None);
        }
    }
}
