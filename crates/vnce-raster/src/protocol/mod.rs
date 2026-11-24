//! Terminal graphics protocols: encoding a [`Canvas`](crate::Canvas) as real pixels.
//!
//! - [`kitty`]: the Kitty graphics protocol (Kitty, `WezTerm`, Ghostty, Konsole, …).
//! - [`sixel`]: Sixel graphics (Windows Terminal, foot, `WezTerm`, xterm, mlterm, …).
//! - [`iterm2`]: the iTerm2 inline image protocol (iTerm2, `WezTerm`, …), behind the `png`
//!   feature.
//!
//! The encoders only produce bytes; placing images on the screen is the job of the
//! terminal layer.

pub mod base64;
#[cfg(feature = "png")]
pub mod iterm2;
pub mod kitty;
pub mod sixel;
