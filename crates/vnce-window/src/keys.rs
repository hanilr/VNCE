//! Keys and modifiers of `winit` as the engine's [`KeyCode`] and [`KeyModifiers`].

use vnce_term::{KeyCode, KeyModifiers};
use winit::keyboard::{Key, ModifiersState, NamedKey};

/// The engine's modifiers for `winit`'s.
pub fn modifiers(state: ModifiersState) -> KeyModifiers {
    let mut out = KeyModifiers::NONE;
    for (on, flag) in [
        (state.shift_key(), KeyModifiers::SHIFT),
        (state.control_key(), KeyModifiers::CONTROL),
        (state.alt_key(), KeyModifiers::ALT),
        (state.super_key(), KeyModifiers::SUPER),
    ] {
        if on {
            out |= flag;
        }
    }
    out
}

/// The engine's key for a `winit` key, or `None` for keys it does not know (modifier
/// keys on their own, media keys).
///
/// Characters come as typed (with Shift: `A`, `!`), as terminals report them; Tab with
/// Shift is `BackTab`.
pub fn key_code(key: &Key, modifiers: KeyModifiers) -> Option<KeyCode> {
    match key {
        Key::Character(text) => text.chars().next().map(KeyCode::Char),
        Key::Named(named) => named_key(*named, modifiers),
        _ => None,
    }
}

fn named_key(key: NamedKey, modifiers: KeyModifiers) -> Option<KeyCode> {
    Some(match key {
        NamedKey::Space => KeyCode::Char(' '),
        NamedKey::Enter => KeyCode::Enter,
        NamedKey::Tab if modifiers.contains(KeyModifiers::SHIFT) => KeyCode::BackTab,
        NamedKey::Tab => KeyCode::Tab,
        NamedKey::Backspace => KeyCode::Backspace,
        NamedKey::Escape => KeyCode::Esc,
        NamedKey::ArrowLeft => KeyCode::Left,
        NamedKey::ArrowRight => KeyCode::Right,
        NamedKey::ArrowUp => KeyCode::Up,
        NamedKey::ArrowDown => KeyCode::Down,
        NamedKey::Home => KeyCode::Home,
        NamedKey::End => KeyCode::End,
        NamedKey::PageUp => KeyCode::PageUp,
        NamedKey::PageDown => KeyCode::PageDown,
        NamedKey::Delete => KeyCode::Delete,
        NamedKey::Insert => KeyCode::Insert,
        NamedKey::CapsLock => KeyCode::CapsLock,
        NamedKey::ScrollLock => KeyCode::ScrollLock,
        NamedKey::NumLock => KeyCode::NumLock,
        NamedKey::PrintScreen => KeyCode::PrintScreen,
        NamedKey::Pause => KeyCode::Pause,
        NamedKey::ContextMenu => KeyCode::Menu,
        NamedKey::F1 => KeyCode::F(1),
        NamedKey::F2 => KeyCode::F(2),
        NamedKey::F3 => KeyCode::F(3),
        NamedKey::F4 => KeyCode::F(4),
        NamedKey::F5 => KeyCode::F(5),
        NamedKey::F6 => KeyCode::F(6),
        NamedKey::F7 => KeyCode::F(7),
        NamedKey::F8 => KeyCode::F(8),
        NamedKey::F9 => KeyCode::F(9),
        NamedKey::F10 => KeyCode::F(10),
        NamedKey::F11 => KeyCode::F(11),
        NamedKey::F12 => KeyCode::F(12),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use winit::keyboard::SmolStr;

    use super::*;

    #[test]
    fn keys_map_like_a_terminal() {
        let none = KeyModifiers::NONE;
        assert_eq!(
            key_code(&Key::Character(SmolStr::new("w")), none),
            Some(KeyCode::Char('w'))
        );
        assert_eq!(
            key_code(&Key::Character(SmolStr::new("A")), KeyModifiers::SHIFT),
            Some(KeyCode::Char('A'))
        );
        assert_eq!(
            key_code(&Key::Named(NamedKey::Space), none),
            Some(KeyCode::Char(' '))
        );
        assert_eq!(
            key_code(&Key::Named(NamedKey::ArrowUp), none),
            Some(KeyCode::Up)
        );
        assert_eq!(
            key_code(&Key::Named(NamedKey::Escape), none),
            Some(KeyCode::Esc)
        );
        assert_eq!(
            key_code(&Key::Named(NamedKey::F11), none),
            Some(KeyCode::F(11))
        );
        assert_eq!(
            key_code(&Key::Named(NamedKey::Tab), KeyModifiers::SHIFT),
            Some(KeyCode::BackTab)
        );
        assert_eq!(key_code(&Key::Named(NamedKey::Shift), none), None);
        let both = modifiers(ModifiersState::SHIFT | ModifiersState::CONTROL);
        assert!(both.contains(KeyModifiers::SHIFT) && both.contains(KeyModifiers::CONTROL));
        assert!(!both.contains(KeyModifiers::ALT));
    }
}
