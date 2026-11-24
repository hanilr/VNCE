//! Platform-specific raw terminal input, used to read answers to terminal queries before
//! the normal event loop starts.

use std::io;
use std::time::{Duration, Instant};

/// Reads raw terminal input until `done` returns `true` or `timeout` expires. Returns an
/// empty answer when the input is not a terminal.
pub(crate) fn read_answer(timeout: Duration, done: &dyn Fn(&[u8]) -> bool) -> io::Result<Vec<u8>> {
    imp::read_answer(Instant::now() + timeout, done)
}

#[cfg(unix)]
mod imp {
    use std::fs::File;
    use std::io;
    use std::os::fd::AsFd;
    use std::time::Instant;

    use rustix::event::{PollFd, PollFlags, Timespec, poll};

    pub(super) fn read_answer(
        deadline: Instant,
        done: &dyn Fn(&[u8]) -> bool,
    ) -> io::Result<Vec<u8>> {
        let stdin = io::stdin();
        // Answers arrive on the controlling terminal; use it directly if stdin is redirected.
        let tty;
        let fd = if rustix::termios::isatty(stdin.as_fd()) {
            stdin.as_fd()
        } else {
            match File::open("/dev/tty") {
                Ok(file) => {
                    tty = file;
                    tty.as_fd()
                }
                Err(_) => return Ok(Vec::new()),
            }
        };

        let mut answer = Vec::new();
        let mut chunk = [0u8; 1024];
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                break;
            }
            let timeout = Timespec {
                tv_sec: i64::try_from(remaining.as_secs()).unwrap_or(i64::MAX),
                tv_nsec: i64::from(remaining.subsec_nanos()),
            };
            let mut fds = [PollFd::new(&fd, PollFlags::IN)];
            match poll(&mut fds, Some(&timeout)) {
                Ok(0) => break,
                Ok(_) => {}
                Err(rustix::io::Errno::INTR) => continue,
                Err(error) => return Err(error.into()),
            }
            match rustix::io::read(fd, &mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    answer.extend_from_slice(&chunk[..read]);
                    if done(&answer) {
                        break;
                    }
                }
                Err(rustix::io::Errno::INTR | rustix::io::Errno::AGAIN) => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(answer)
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::time::Instant;

    use windows_sys::Win32::Foundation::{HANDLE, WAIT_OBJECT_0};
    use windows_sys::Win32::System::Console::{
        ENABLE_VIRTUAL_TERMINAL_INPUT, GetConsoleMode, GetStdHandle, INPUT_RECORD, KEY_EVENT,
        ReadConsoleInputW, STD_INPUT_HANDLE, SetConsoleMode,
    };
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    /// Restores the console input mode when dropped.
    struct ModeGuard {
        handle: HANDLE,
        mode: u32,
    }

    impl Drop for ModeGuard {
        fn drop(&mut self) {
            // SAFETY: `handle` is the process's console input handle, which stays valid for
            // the whole process; restoring a previously read mode is always allowed.
            unsafe {
                SetConsoleMode(self.handle, self.mode);
            }
        }
    }

    pub(super) fn read_answer(
        deadline: Instant,
        done: &dyn Fn(&[u8]) -> bool,
    ) -> io::Result<Vec<u8>> {
        // SAFETY: GetStdHandle has no preconditions.
        let handle = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        let mut mode = 0;
        // SAFETY: `mode` is a valid pointer for the duration of the call. The call fails
        // (returns 0) when the handle is not a console, e.g. when input is redirected.
        if handle.is_null() || unsafe { GetConsoleMode(handle, &raw mut mode) } == 0 {
            return Ok(Vec::new());
        }
        // Answers only arrive as text when virtual terminal input is enabled.
        let _guard = ModeGuard { handle, mode };
        // SAFETY: `handle` is a valid console input handle (checked above).
        unsafe {
            SetConsoleMode(handle, mode | ENABLE_VIRTUAL_TERMINAL_INPUT);
        }

        let mut units: Vec<u16> = Vec::new();
        let mut answer = Vec::new();
        let mut records = [INPUT_RECORD::default(); 64];
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            let millis = u32::try_from(remaining.as_millis()).unwrap_or(u32::MAX);
            if millis == 0 {
                break;
            }
            // SAFETY: `handle` is a valid console input handle.
            if unsafe { WaitForSingleObject(handle, millis) } != WAIT_OBJECT_0 {
                break;
            }
            let mut read = 0;
            let capacity = u32::try_from(records.len()).unwrap_or(0);
            // SAFETY: `records` has room for `capacity` records and `read` is a valid pointer.
            let ok =
                unsafe { ReadConsoleInputW(handle, records.as_mut_ptr(), capacity, &raw mut read) };
            if ok == 0 {
                return Err(io::Error::last_os_error());
            }
            for record in &records[..usize::try_from(read).unwrap_or(0)] {
                if u32::from(record.EventType) != KEY_EVENT {
                    continue;
                }
                // SAFETY: `EventType` is KEY_EVENT, so the `KeyEvent` variant is active.
                let key = unsafe { record.Event.KeyEvent };
                if key.bKeyDown == 0 {
                    continue;
                }
                // SAFETY: both union variants are plain integers; UnicodeChar is the one
                // filled by ReadConsoleInputW.
                let unit = unsafe { key.uChar.UnicodeChar };
                if unit != 0 {
                    for _ in 0..key.wRepeatCount.max(1) {
                        units.push(unit);
                    }
                }
            }
            answer = String::from_utf16_lossy(&units).into_bytes();
            if done(&answer) {
                break;
            }
        }
        Ok(answer)
    }
}
