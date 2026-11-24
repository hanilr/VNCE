//! Action queries inside a running game (presses and releases are only visible during the
//! update that follows them).

use std::time::Duration;

use vnce_core::Size;
use vnce_game::ActionMap;
use vnce_runtime::{Context, Game, RunConfig, ScriptedDriver, run_with};
use vnce_term::{
    Capabilities, Event, Frame, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, Terminal,
    TestBackend,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Jump,
}

struct Probe {
    controls: ActionMap<Action>,
    presses: u32,
    releases: u32,
}

impl Game for Probe {
    fn update(&mut self, ctx: &mut Context) {
        if self.controls.was_pressed(Action::Jump, ctx.input()) {
            self.presses += 1;
        }
        if self.controls.was_released(Action::Jump, ctx.input()) {
            self.releases += 1;
        }
        if ctx.elapsed() > Duration::from_secs(1) {
            ctx.quit();
        }
    }

    fn render(&mut self, _ctx: &Context, _frame: &mut Frame<'_>) {}
}

fn key(code: KeyCode, kind: KeyEventKind) -> Event {
    Event::Key(KeyEvent {
        code,
        modifiers: KeyModifiers::NONE,
        kind,
    })
}

#[test]
fn presses_and_releases_are_seen_once() {
    let mut driver = ScriptedDriver::new();
    for (at, code, kind) in [
        (100, KeyCode::Char(' '), KeyEventKind::Press),
        (300, KeyCode::Char(' '), KeyEventKind::Release),
        (500, KeyCode::Up, KeyEventKind::Press),
        (600, KeyCode::Up, KeyEventKind::Release),
    ] {
        driver.push_event(Duration::from_millis(at), key(code, kind));
    }
    let size = Size::new(10, 3);
    let capabilities = Capabilities {
        key_release_events: true,
        ..Capabilities::default()
    };
    let terminal =
        Terminal::new(TestBackend::new(size.width, size.height), capabilities).expect("terminal");
    let mut probe = Probe {
        controls: ActionMap::new()
            .bind(Action::Jump, ' ')
            .bind(Action::Jump, KeyCode::Up),
        presses: 0,
        releases: 0,
    };
    run_with(&mut probe, RunConfig::default(), terminal, driver).expect("run");
    assert_eq!((probe.presses, probe.releases), (2, 2));
}
