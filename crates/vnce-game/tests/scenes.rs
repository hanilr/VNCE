//! The scene stack in a running game: push, pop, switch, quit and overlays.

use std::cell::RefCell;
use std::time::Duration;

use vnce_game::{Scene, SceneStack, Transition};
use vnce_runtime::{Context, RunConfig, ScriptedDriver, run_with};
use vnce_term::{
    Capabilities, Event, Frame, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, Terminal,
    TestBackend,
};

/// What the scenes did, shared through the stack.
#[derive(Default)]
struct Log {
    events: Vec<&'static str>,
    /// The scenes drawn in each frame, by frame number (render only gets `&Log`).
    frames: RefCell<Vec<(u64, Vec<&'static str>)>>,
}

fn pressed(event: &Event) -> Option<char> {
    match event {
        Event::Key(KeyEvent {
            code: KeyCode::Char(c),
            kind: KeyEventKind::Press,
            ..
        }) => Some(*c),
        _ => None,
    }
}

fn draw(log: &Log, name: &'static str, frame: &Frame<'_>) {
    let mut frames = log.frames.borrow_mut();
    if frames
        .last()
        .is_none_or(|(count, _)| *count != frame.count())
    {
        frames.push((frame.count(), Vec::new()));
    }
    frames.last_mut().expect("frame").1.push(name);
}

struct Title;
struct Pause;
struct Play;

impl Scene<Log> for Title {
    fn start(&mut self, log: &mut Log, _ctx: &mut Context) {
        log.events.push("start title");
    }
    fn resume(&mut self, log: &mut Log, _ctx: &mut Context) {
        log.events.push("resume title");
    }
    fn event(&mut self, _log: &mut Log, _ctx: &mut Context, event: &Event) -> Transition<Log> {
        match pressed(event) {
            Some('p') => Transition::push(Pause),
            Some('s') => Transition::switch(Play),
            _ => Transition::None,
        }
    }
    fn update(&mut self, _log: &mut Log, _ctx: &mut Context) -> Transition<Log> {
        Transition::None
    }
    fn render(&mut self, log: &Log, _ctx: &Context, frame: &mut Frame<'_>) {
        draw(log, "title", frame);
    }
}

impl Scene<Log> for Pause {
    fn start(&mut self, log: &mut Log, _ctx: &mut Context) {
        log.events.push("start pause");
    }
    fn event(&mut self, _log: &mut Log, _ctx: &mut Context, event: &Event) -> Transition<Log> {
        if pressed(event) == Some('p') {
            Transition::Pop
        } else {
            Transition::None
        }
    }
    fn update(&mut self, _log: &mut Log, _ctx: &mut Context) -> Transition<Log> {
        Transition::None
    }
    fn render(&mut self, log: &Log, _ctx: &Context, frame: &mut Frame<'_>) {
        draw(log, "pause", frame);
    }
    fn is_overlay(&self) -> bool {
        true
    }
}

impl Scene<Log> for Play {
    fn start(&mut self, log: &mut Log, _ctx: &mut Context) {
        log.events.push("start play");
    }
    fn event(&mut self, _log: &mut Log, _ctx: &mut Context, event: &Event) -> Transition<Log> {
        if pressed(event) == Some('q') {
            Transition::Quit
        } else {
            Transition::None
        }
    }
    fn update(&mut self, log: &mut Log, _ctx: &mut Context) -> Transition<Log> {
        if log.events.last() != Some(&"play update") {
            log.events.push("play update");
        }
        Transition::None
    }
    fn render(&mut self, log: &Log, _ctx: &Context, frame: &mut Frame<'_>) {
        draw(log, "play", frame);
    }
}

#[test]
fn push_pop_switch_and_quit() {
    let mut driver = ScriptedDriver::new();
    for (at, c) in [(100, 'p'), (300, 'p'), (500, 's'), (700, 'q')] {
        driver.push_event(
            Duration::from_millis(at),
            Event::Key(KeyEvent {
                code: KeyCode::Char(c),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
            }),
        );
    }
    let terminal =
        Terminal::new(TestBackend::new(10, 3), Capabilities::default()).expect("terminal");
    let mut game = SceneStack::new(Title, Log::default());
    assert_eq!(game.len(), 1);
    run_with(&mut game, RunConfig::default(), terminal, driver).expect("run");

    assert_eq!(
        game.data().events,
        [
            "start title",
            "start pause",
            "resume title",
            "start play",
            "play update"
        ]
    );
    let frames: Vec<Vec<&str>> = game
        .data()
        .frames
        .borrow()
        .iter()
        .map(|(_, f)| f.clone())
        .collect();
    // The pause menu is drawn over the title, which stays visible.
    assert!(frames.iter().any(|f| f == &["title", "pause"]));
    assert!(frames.iter().any(|f| f == &["title"]));
    assert_eq!(frames.last().map(Vec::as_slice), Some(&["play"][..]));
    // The title is gone after the switch.
    let switched = frames
        .iter()
        .position(|f| f == &["play"])
        .expect("play frame");
    assert!(frames[switched..].iter().all(|f| f == &["play"]));
}
