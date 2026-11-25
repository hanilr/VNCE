//! Keyboard, mouse, resize, focus and paste events.
//!
//! The event types are defined here so that applications never depend on the underlying
//! terminal library directly.
//!
//! Platform notes:
//!
//! - On Windows, both key presses and key releases are reported
//!   ([`KeyEventKind::Release`]). On other platforms usually only presses are reported.
//!   Check [`KeyEvent::is_press`] when an action should happen once per key press.
//! - In raw mode `Ctrl+C` does not stop the program; it arrives as a key event.

use std::fmt;
use std::io;
use std::ops::{BitOr, BitOrAssign};
use std::time::{Duration, Instant};

use crossterm::event as ct;
use vnce_core::Size;

/// A terminal event.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Event {
    /// A key was pressed, repeated or released.
    Key(KeyEvent),
    /// A mouse event. Requires [`TerminalOptions::mouse_capture`](crate::TerminalOptions).
    Mouse(MouseEvent),
    /// The terminal was resized to the given size.
    Resize(Size),
    /// The terminal window gained focus.
    /// Requires [`TerminalOptions::focus_events`](crate::TerminalOptions).
    FocusGained,
    /// The terminal window lost focus.
    /// Requires [`TerminalOptions::focus_events`](crate::TerminalOptions).
    FocusLost,
    /// Text was pasted.
    /// Requires [`TerminalOptions::bracketed_paste`](crate::TerminalOptions).
    Paste(String),
}

/// A keyboard event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    /// The key.
    pub code: KeyCode,
    /// Modifier keys held down.
    pub modifiers: KeyModifiers,
    /// Press, repeat or release.
    pub kind: KeyEventKind,
}

impl KeyEvent {
    /// Returns `true` for presses and repeats (not releases).
    #[must_use]
    pub fn is_press(&self) -> bool {
        matches!(self.kind, KeyEventKind::Press | KeyEventKind::Repeat)
    }
}

/// A key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyCode {
    /// A character key. Letters are reported in their shifted form when Shift is held.
    Char(char),
    /// A function key (`F(1)` is F1).
    F(u8),
    /// Backspace.
    Backspace,
    /// Enter / Return.
    Enter,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Home.
    Home,
    /// End.
    End,
    /// Page up.
    PageUp,
    /// Page down.
    PageDown,
    /// Tab.
    Tab,
    /// Shift + Tab.
    BackTab,
    /// Delete.
    Delete,
    /// Insert.
    Insert,
    /// Escape.
    Esc,
    /// Caps Lock.
    CapsLock,
    /// Scroll Lock.
    ScrollLock,
    /// Num Lock.
    NumLock,
    /// Print Screen.
    PrintScreen,
    /// Pause.
    Pause,
    /// Menu.
    Menu,
    /// The "begin" key (keypad 5 with Num Lock off).
    KeypadBegin,
    /// The null key (often `Ctrl+Space`).
    Null,
}

/// Names of the keys without a character, as used by [`KeyCode`]'s `Display` and `FromStr`.
const KEY_NAMES: [(KeyCode, &str); 23] = [
    (KeyCode::Backspace, "Backspace"),
    (KeyCode::Enter, "Enter"),
    (KeyCode::Left, "Left"),
    (KeyCode::Right, "Right"),
    (KeyCode::Up, "Up"),
    (KeyCode::Down, "Down"),
    (KeyCode::Home, "Home"),
    (KeyCode::End, "End"),
    (KeyCode::PageUp, "PageUp"),
    (KeyCode::PageDown, "PageDown"),
    (KeyCode::Tab, "Tab"),
    (KeyCode::BackTab, "BackTab"),
    (KeyCode::Delete, "Delete"),
    (KeyCode::Insert, "Insert"),
    (KeyCode::Esc, "Esc"),
    (KeyCode::CapsLock, "CapsLock"),
    (KeyCode::ScrollLock, "ScrollLock"),
    (KeyCode::NumLock, "NumLock"),
    (KeyCode::PrintScreen, "PrintScreen"),
    (KeyCode::Pause, "Pause"),
    (KeyCode::Menu, "Menu"),
    (KeyCode::KeypadBegin, "KeypadBegin"),
    (KeyCode::Null, "Null"),
];

impl fmt::Display for KeyCode {
    /// A short name that [`FromStr`](std::str::FromStr) reads back: the character itself
    /// (`w`, `?`), `Space`, `F5`, or the key name (`Up`, `Enter`, `Esc`, ...).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::Char(' ') => f.write_str("Space"),
            Self::Char(c) => write!(f, "{c}"),
            Self::F(n) => write!(f, "F{n}"),
            key => {
                let name = KEY_NAMES
                    .iter()
                    .find(|(k, _)| *k == key)
                    .map_or("?", |(_, name)| name);
                f.write_str(name)
            }
        }
    }
}

/// The error returned when a key name cannot be parsed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseKeyCodeError(String);

impl fmt::Display for ParseKeyCodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "unknown key name {:?}", self.0)
    }
}

impl std::error::Error for ParseKeyCodeError {}

impl std::str::FromStr for KeyCode {
    type Err = ParseKeyCodeError;

    /// Parses a name written by `Display`. A single character is that character key;
    /// longer names ignore case (`esc`, `F5`, `pageup`, `space`).
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars();
        if let (Some(c), None) = (chars.next(), chars.next()) {
            return Ok(Self::Char(c));
        }
        let name = s.trim();
        if name.eq_ignore_ascii_case("space") {
            return Ok(Self::Char(' '));
        }
        if let Some(&(key, _)) = KEY_NAMES.iter().find(|(_, n)| n.eq_ignore_ascii_case(name)) {
            return Ok(key);
        }
        name.strip_prefix(['F', 'f'])
            .and_then(|number| number.parse().ok())
            .map(Self::F)
            .ok_or_else(|| ParseKeyCodeError(s.to_owned()))
    }
}

/// Whether a key was pressed, repeated (held down) or released.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyEventKind {
    /// The key was pressed.
    Press,
    /// The key is held down and repeats.
    Repeat,
    /// The key was released.
    Release,
}

/// Modifier keys (Shift, Control, ...).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct KeyModifiers(u8);

impl KeyModifiers {
    /// No modifier.
    pub const NONE: Self = Self(0);
    /// Shift.
    pub const SHIFT: Self = Self(1);
    /// Control.
    pub const CONTROL: Self = Self(1 << 1);
    /// Alt (Option on macOS).
    pub const ALT: Self = Self(1 << 2);
    /// Super (Windows key, Command on macOS).
    pub const SUPER: Self = Self(1 << 3);
    /// Hyper.
    pub const HYPER: Self = Self(1 << 4);
    /// Meta.
    pub const META: Self = Self(1 << 5);

    const NAMED: [(Self, &'static str); 6] = [
        (Self::SHIFT, "SHIFT"),
        (Self::CONTROL, "CONTROL"),
        (Self::ALT, "ALT"),
        (Self::SUPER, "SUPER"),
        (Self::HYPER, "HYPER"),
        (Self::META, "META"),
    ];

    /// Returns `true` if no modifier is held.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// Returns `true` if all modifiers in `other` are held.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Modifiers held in either `self` or `other`.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl fmt::Debug for KeyModifiers {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("KeyModifiers(NONE)");
        }
        f.write_str("KeyModifiers(")?;
        let mut first = true;
        for (modifier, name) in Self::NAMED {
            if self.contains(modifier) {
                if !first {
                    f.write_str(" | ")?;
                }
                f.write_str(name)?;
                first = false;
            }
        }
        f.write_str(")")
    }
}

impl BitOr for KeyModifiers {
    type Output = Self;
    fn bitor(self, rhs: Self) -> Self {
        self.union(rhs)
    }
}

impl BitOrAssign for KeyModifiers {
    fn bitor_assign(&mut self, rhs: Self) {
        *self = self.union(rhs);
    }
}

/// A mouse event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MouseEvent {
    /// What happened.
    pub kind: MouseEventKind,
    /// Column of the mouse pointer (0-based).
    pub column: u16,
    /// Row of the mouse pointer (0-based).
    pub row: u16,
    /// Modifier keys held down.
    pub modifiers: KeyModifiers,
}

/// The kind of a mouse event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseEventKind {
    /// A button was pressed.
    Down(MouseButton),
    /// A button was released.
    Up(MouseButton),
    /// The mouse moved while a button was held.
    Drag(MouseButton),
    /// The mouse moved with no button held.
    Moved,
    /// The wheel scrolled down.
    ScrollDown,
    /// The wheel scrolled up.
    ScrollUp,
    /// The wheel scrolled left.
    ScrollLeft,
    /// The wheel scrolled right.
    ScrollRight,
}

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MouseButton {
    /// Left button.
    Left,
    /// Right button.
    Right,
    /// Middle button (wheel click).
    Middle,
}

/// Waits up to `timeout` for an event. Returns `Ok(None)` if no event arrived.
///
/// Use `Duration::ZERO` to check for pending events without waiting.
///
/// On Windows the wait ends within about a millisecond of the timeout, although the system
/// waits for console input in steps of about 15.6 ms.
///
/// # Errors
///
/// Returns an error if reading from the terminal fails.
pub fn poll(timeout: Duration) -> io::Result<Option<Event>> {
    let deadline = Instant::now().checked_add(timeout);
    loop {
        let remaining = deadline.map_or(timeout, |d| d.saturating_duration_since(Instant::now()));
        if !wait_for_event(remaining)? {
            return Ok(None);
        }
        // Events we do not report (media keys, modifier-only keys) are skipped.
        if let Some(event) = convert_event(ct::read()?) {
            return Ok(Some(event));
        }
        if remaining.is_zero() {
            return Ok(None);
        }
    }
}

/// Waits up to `timeout` until an event can be read.
fn wait_for_event(timeout: Duration) -> io::Result<bool> {
    #[cfg(windows)]
    {
        precise_wait(timeout, ct::poll, std::thread::sleep)
    }
    #[cfg(not(windows))]
    {
        ct::poll(timeout)
    }
}

/// Longest wait that is rounded up to the next step of the Windows system timer (15.6 ms
/// by default): waits for console input with a timeout end on those steps only.
#[cfg(any(windows, test))]
const COARSE_STEP: Duration = Duration::from_millis(16);

/// Length of the short sleeps at the end of a wait.
#[cfg(any(windows, test))]
const FINE_STEP: Duration = Duration::from_millis(1);

/// Waits up to `timeout` for `poll` to report an event, ending close to the timeout even
/// when `poll` only wakes up on coarse timer steps (Windows: a 1 ms wait can take 15 ms,
/// which makes a 60 FPS game alternate between 15.6 ms and 31 ms frames).
///
/// `poll` waits coarsely while more than [`COARSE_STEP`] remains; the rest is spent in short
/// `sleep`s (the standard library sleeps with a high-resolution timer), checking for events
/// in between, so an event waits at most [`FINE_STEP`].
#[cfg(any(windows, test))]
fn precise_wait(
    timeout: Duration,
    mut poll: impl FnMut(Duration) -> io::Result<bool>,
    mut sleep: impl FnMut(Duration),
) -> io::Result<bool> {
    let Some(deadline) = Instant::now().checked_add(timeout) else {
        // No deadline that can be represented: wait as long as `poll` does.
        return poll(timeout);
    };
    loop {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining > COARSE_STEP {
            if poll(remaining.saturating_sub(COARSE_STEP))? {
                return Ok(true);
            }
            continue;
        }
        if poll(Duration::ZERO)? {
            return Ok(true);
        }
        if remaining.is_zero() {
            return Ok(false);
        }
        sleep(remaining.min(FINE_STEP));
    }
}

/// Blocks until an event arrives.
///
/// # Errors
///
/// Returns an error if reading from the terminal fails.
pub fn read() -> io::Result<Event> {
    loop {
        if let Some(event) = convert_event(ct::read()?) {
            return Ok(event);
        }
    }
}

/// Converts a `crossterm` event. Returns `None` for events that are not reported.
fn convert_event(event: ct::Event) -> Option<Event> {
    Some(match event {
        ct::Event::Key(key) => Event::Key(KeyEvent {
            code: convert_key_code(key.code)?,
            modifiers: convert_modifiers(key.modifiers),
            kind: match key.kind {
                ct::KeyEventKind::Press => KeyEventKind::Press,
                ct::KeyEventKind::Repeat => KeyEventKind::Repeat,
                ct::KeyEventKind::Release => KeyEventKind::Release,
            },
        }),
        ct::Event::Mouse(mouse) => Event::Mouse(MouseEvent {
            kind: match mouse.kind {
                ct::MouseEventKind::Down(button) => MouseEventKind::Down(convert_button(button)),
                ct::MouseEventKind::Up(button) => MouseEventKind::Up(convert_button(button)),
                ct::MouseEventKind::Drag(button) => MouseEventKind::Drag(convert_button(button)),
                ct::MouseEventKind::Moved => MouseEventKind::Moved,
                ct::MouseEventKind::ScrollDown => MouseEventKind::ScrollDown,
                ct::MouseEventKind::ScrollUp => MouseEventKind::ScrollUp,
                ct::MouseEventKind::ScrollLeft => MouseEventKind::ScrollLeft,
                ct::MouseEventKind::ScrollRight => MouseEventKind::ScrollRight,
            },
            column: mouse.column,
            row: mouse.row,
            modifiers: convert_modifiers(mouse.modifiers),
        }),
        ct::Event::Resize(width, height) => Event::Resize(Size::new(width, height)),
        ct::Event::FocusGained => Event::FocusGained,
        ct::Event::FocusLost => Event::FocusLost,
        ct::Event::Paste(text) => Event::Paste(text),
    })
}

fn convert_key_code(code: ct::KeyCode) -> Option<KeyCode> {
    Some(match code {
        ct::KeyCode::Char(c) => KeyCode::Char(c),
        ct::KeyCode::F(n) => KeyCode::F(n),
        ct::KeyCode::Backspace => KeyCode::Backspace,
        ct::KeyCode::Enter => KeyCode::Enter,
        ct::KeyCode::Left => KeyCode::Left,
        ct::KeyCode::Right => KeyCode::Right,
        ct::KeyCode::Up => KeyCode::Up,
        ct::KeyCode::Down => KeyCode::Down,
        ct::KeyCode::Home => KeyCode::Home,
        ct::KeyCode::End => KeyCode::End,
        ct::KeyCode::PageUp => KeyCode::PageUp,
        ct::KeyCode::PageDown => KeyCode::PageDown,
        ct::KeyCode::Tab => KeyCode::Tab,
        ct::KeyCode::BackTab => KeyCode::BackTab,
        ct::KeyCode::Delete => KeyCode::Delete,
        ct::KeyCode::Insert => KeyCode::Insert,
        ct::KeyCode::Esc => KeyCode::Esc,
        ct::KeyCode::CapsLock => KeyCode::CapsLock,
        ct::KeyCode::ScrollLock => KeyCode::ScrollLock,
        ct::KeyCode::NumLock => KeyCode::NumLock,
        ct::KeyCode::PrintScreen => KeyCode::PrintScreen,
        ct::KeyCode::Pause => KeyCode::Pause,
        ct::KeyCode::Menu => KeyCode::Menu,
        ct::KeyCode::KeypadBegin => KeyCode::KeypadBegin,
        ct::KeyCode::Null => KeyCode::Null,
        ct::KeyCode::Media(_) | ct::KeyCode::Modifier(_) => return None,
    })
}

fn convert_modifiers(modifiers: ct::KeyModifiers) -> KeyModifiers {
    let pairs = [
        (ct::KeyModifiers::SHIFT, KeyModifiers::SHIFT),
        (ct::KeyModifiers::CONTROL, KeyModifiers::CONTROL),
        (ct::KeyModifiers::ALT, KeyModifiers::ALT),
        (ct::KeyModifiers::SUPER, KeyModifiers::SUPER),
        (ct::KeyModifiers::HYPER, KeyModifiers::HYPER),
        (ct::KeyModifiers::META, KeyModifiers::META),
    ];
    pairs
        .into_iter()
        .filter(|(from, _)| modifiers.contains(*from))
        .fold(KeyModifiers::NONE, |acc, (_, to)| acc | to)
}

fn convert_button(button: ct::MouseButton) -> MouseButton {
    match button {
        ct::MouseButton::Left => MouseButton::Left,
        ct::MouseButton::Right => MouseButton::Right,
        ct::MouseButton::Middle => MouseButton::Middle,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precise_wait_ends_on_time_with_a_coarse_poll() {
        // A poll that, like Windows, rounds every non-zero wait up to 16 ms steps. The test
        // checks what `precise_wait` asks for, not how long sleeping takes: CI machines
        // (macOS in particular) can oversleep by many milliseconds.
        for millis in [0, 1, 5, 17, 40] {
            let timeout = Duration::from_millis(millis);
            let start = Instant::now();
            let deadline = start + timeout;
            let mut coarse_waits = Vec::new();
            let coarse_poll = |wait: Duration| -> io::Result<bool> {
                if !wait.is_zero() {
                    coarse_waits.push((Instant::now(), wait));
                    let steps = wait.as_micros().div_ceil(16_000);
                    let micros = u64::try_from(steps * 16_000).unwrap_or(u64::MAX);
                    std::thread::sleep(Duration::from_micros(micros));
                }
                Ok(false)
            };
            assert!(!precise_wait(timeout, coarse_poll, std::thread::sleep).unwrap());
            let elapsed = start.elapsed();
            assert!(elapsed >= timeout, "{millis} ms: {elapsed:?}");
            // A coarse wait, even rounded up to the next 16 ms step, never reaches past the
            // deadline: only the short sleeps of the last 16 ms are left. (`at` is taken a
            // moment after `precise_wait` computed the wait, hence the small margin.)
            let margin = Duration::from_millis(5);
            for (at, wait) in coarse_waits {
                assert!(
                    at + wait + COARSE_STEP <= deadline + margin,
                    "{millis} ms: {wait:?}"
                );
            }
            // Generous bound: only catches a wait that does not end.
            assert!(
                elapsed < timeout + Duration::from_secs(1),
                "{millis} ms: {elapsed:?}"
            );
        }
    }

    #[test]
    fn precise_wait_returns_as_soon_as_an_event_is_ready() {
        let mut calls = 0;
        let ready = precise_wait(
            Duration::from_secs(10),
            |_| {
                calls += 1;
                Ok(calls == 3)
            },
            |_| {},
        );
        assert!(ready.unwrap());
        assert_eq!(calls, 3);
        // Errors from the poll are returned.
        let error = precise_wait(
            Duration::from_millis(1),
            |_| Err(io::Error::other("closed")),
            |_| {},
        );
        assert!(error.is_err());
    }

    #[test]
    fn key_names_round_trip() {
        let mut keys: Vec<KeyCode> = KEY_NAMES.iter().map(|&(k, _)| k).collect();
        keys.extend([
            KeyCode::Char('w'),
            KeyCode::Char('W'),
            KeyCode::Char(' '),
            KeyCode::Char('#'),
            KeyCode::Char('='),
            KeyCode::Char('ş'),
            KeyCode::F(1),
            KeyCode::F(12),
        ]);
        for key in keys {
            let name = key.to_string();
            assert_eq!(name.parse::<KeyCode>(), Ok(key), "{name}");
        }
    }

    #[test]
    fn key_names_ignore_case() {
        assert_eq!("esc".parse(), Ok(KeyCode::Esc));
        assert_eq!("PAGEUP".parse(), Ok(KeyCode::PageUp));
        assert_eq!("f5".parse(), Ok(KeyCode::F(5)));
        assert_eq!("space".parse(), Ok(KeyCode::Char(' ')));
        assert_eq!(KeyCode::Char(' ').to_string(), "Space");
        assert!("".parse::<KeyCode>().is_err());
        assert!("Nope".parse::<KeyCode>().is_err());
        assert!("F999".parse::<KeyCode>().is_err());
    }

    #[test]
    fn key_events_are_converted() {
        let key = ct::KeyEvent::new_with_kind(
            ct::KeyCode::Char('a'),
            ct::KeyModifiers::CONTROL | ct::KeyModifiers::SHIFT,
            ct::KeyEventKind::Release,
        );
        let event = convert_event(ct::Event::Key(key));
        assert_eq!(
            event,
            Some(Event::Key(KeyEvent {
                code: KeyCode::Char('a'),
                modifiers: KeyModifiers::CONTROL | KeyModifiers::SHIFT,
                kind: KeyEventKind::Release,
            }))
        );
    }

    #[test]
    fn unsupported_keys_are_skipped() {
        let key = ct::KeyEvent::new(
            ct::KeyCode::Media(ct::MediaKeyCode::Play),
            ct::KeyModifiers::NONE,
        );
        assert_eq!(convert_event(ct::Event::Key(key)), None);
    }

    #[test]
    fn mouse_resize_focus_and_paste_are_converted() {
        let mouse = ct::MouseEvent {
            kind: ct::MouseEventKind::Drag(ct::MouseButton::Right),
            column: 3,
            row: 4,
            modifiers: ct::KeyModifiers::ALT,
        };
        assert_eq!(
            convert_event(ct::Event::Mouse(mouse)),
            Some(Event::Mouse(MouseEvent {
                kind: MouseEventKind::Drag(MouseButton::Right),
                column: 3,
                row: 4,
                modifiers: KeyModifiers::ALT,
            }))
        );
        assert_eq!(
            convert_event(ct::Event::Resize(80, 24)),
            Some(Event::Resize(Size::new(80, 24)))
        );
        assert_eq!(convert_event(ct::Event::FocusLost), Some(Event::FocusLost));
        assert_eq!(
            convert_event(ct::Event::Paste("hi".into())),
            Some(Event::Paste("hi".into()))
        );
    }

    #[test]
    fn key_event_is_press() {
        let mut key = KeyEvent {
            code: KeyCode::Enter,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
        };
        assert!(key.is_press());
        key.kind = KeyEventKind::Repeat;
        assert!(key.is_press());
        key.kind = KeyEventKind::Release;
        assert!(!key.is_press());
    }

    #[test]
    fn key_modifiers_debug() {
        assert_eq!(
            format!("{:?}", KeyModifiers::CONTROL | KeyModifiers::ALT),
            "KeyModifiers(CONTROL | ALT)"
        );
        assert_eq!(format!("{:?}", KeyModifiers::NONE), "KeyModifiers(NONE)");
    }
}
