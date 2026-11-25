//! The title (over a flight by the autopilot), the course code, pause, settings, controls
//! and end scenes.
//!
//! Menus shown over another scene never replace the scene below them (a `Switch` only
//! replaces the top scene): they leave a [`Request`] in the shared data and close, and the
//! scene below acts on it.

use vnce::vnce_core::{Color, Rect, Style};
use vnce::vnce_game::{ActionMap, Binding, Rng, Scene, Transition};
use vnce::vnce_runtime::{Context, FpsLimit};
use vnce::vnce_term::{Event, Frame, KeyCode, KeyEventKind};
use vnce::vnce_ui::{
    Block, BorderType, Clear, Line, List, ListState, Padding, StatefulWidget, Widget,
};

use crate::audio::{Mood, Sfx};
use crate::course::{self, CODE_LENGTH, CODE_LETTERS, CODES};
use crate::play::{Outcome, Play};
use crate::{
    ACCENT, ACTIONS, Action, COOL, FRAME_RATES, Mode, PANEL, RESOLUTIONS, Request, Shared,
    default_controls, format_time, help_line, popup, status_line,
};

/// A menu list in a rounded block.
fn menu<'a>(items: Vec<Line<'a>>, title: &'a str, bottom: Line<'a>, accent: Color) -> List<'a> {
    List::new(items)
        .highlight_symbol("▶ ")
        .highlight_style(Style::new().fg(Color::rgb(16, 18, 26)).bg(accent))
        .style(Style::new().bg(PANEL))
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .border_style(Style::new().fg(accent))
                .title(Line::from(title).centered())
                .title_bottom(bottom.centered())
                .padding(Padding::new(2, 2, 1, 1)),
        )
}

/// Moves a menu selection with the up / down actions.
fn navigate(shared: &Shared, ctx: &Context, state: &mut ListState, rows: usize) {
    let input = ctx.input();
    if shared.controls.was_pressed(Action::Up, input) {
        state.select_previous(rows, true);
    }
    if shared.controls.was_pressed(Action::Down, input) {
        state.select_next(rows, true);
    }
}

/// Moves `index` by `direction` through `length` items, wrapping around.
fn step(index: usize, direction: i32, length: usize) -> usize {
    (index as i32 + direction).rem_euclid(length as i32) as usize
}

/// A new course seed.
fn random_seed() -> u64 {
    Rng::from_time().next_u64() % CODES
}

// ---------------------------------------------------------------------------------------
// Title
// ---------------------------------------------------------------------------------------

pub struct Title {
    menu: ListState,
    backdrop: Play,
}

impl Title {
    pub fn new(shared: &Shared) -> Self {
        Self {
            menu: ListState::with_selected(Some(0)),
            backdrop: Play::demo(shared.last),
        }
    }
}

const TITLE_MENU: [&str; 6] = [
    "Ring course",
    "Free flight",
    "Replay last course",
    "Course code…",
    "Settings",
    "Quit",
];

impl Scene<Shared> for Title {
    fn start(&mut self, shared: &mut Shared, ctx: &mut Context) {
        shared.sounds.engine(false);
        shared.sounds.music(Mood::Menu);
        shared.detect_modes(ctx);
    }

    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        if let Some(Request::Play(mode, seed)) = shared.request.take() {
            return Transition::switch(Play::new(mode, seed));
        }
        self.backdrop.step_backdrop(shared, ctx.dt_secs());
        navigate(shared, ctx, &mut self.menu, TITLE_MENU.len());
        let input = ctx.input();
        if shared.controls.was_pressed(Action::Method, input) {
            shared.cycle_method(ctx);
        }
        if shared.controls.was_pressed(Action::Quit, input) {
            shared.store(ctx);
            return Transition::Quit;
        }
        if shared.controls.was_pressed(Action::Confirm, input) {
            shared.sounds.play(Sfx::Blip);
            return match self.menu.selected() {
                Some(0) => Transition::switch(Play::new(Mode::Course, random_seed())),
                Some(1) => Transition::switch(Play::new(Mode::Free, random_seed())),
                Some(2) => Transition::switch(Play::new(Mode::Course, shared.last)),
                Some(3) => Transition::push(Code::new()),
                Some(4) => Transition::push(Settings::new()),
                _ => {
                    shared.store(ctx);
                    Transition::Quit
                }
            };
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, _ctx: &Context, frame: &mut Frame<'_>) {
        self.backdrop.backdrop(shared, frame);
        status_line(
            frame,
            &format!(
                " Glide · {} · {} · sound: {}",
                frame.graphics_mode(),
                shared.renderer.borrow().describe(),
                shared.sounds.device()
            ),
        );
        let best = match shared.best_time(shared.last) {
            0 if shared.best_free == 0 => String::new(),
            0 => format!(" best free flight {} rings ", shared.best_free),
            time => format!(
                " best on {} {} ",
                course::code(shared.last),
                format_time(time)
            ),
        };
        let area = frame.area();
        let centered = area.centered(36, TITLE_MENU.len() as u16 + 4);
        let menu_area = Rect {
            y: (centered.y + area.height / 6).min(area.height.saturating_sub(centered.height + 1)),
            ..centered
        };
        let buffer = frame.buffer_mut();
        // The name in big letters above the menu.
        let name = "G L I D E";
        let x = area.width.saturating_sub(name.len() as u16) / 2;
        let y = menu_area.y.saturating_sub(3).max(1);
        buffer.set_string(x, y, name, Style::new().fg(ACCENT).bg(PANEL).bold());
        let subtitle = " a vnce showcase in 3D ";
        let x = area.width.saturating_sub(subtitle.chars().count() as u16) / 2;
        buffer.set_string(x, y + 1, subtitle, Style::new().fg(COOL).bg(PANEL));
        Clear.render(menu_area, buffer);
        let items = TITLE_MENU
            .iter()
            .enumerate()
            .map(|(i, &item)| {
                if i == 2 {
                    Line::from(format!("Replay course {}", course::code(shared.last)))
                } else {
                    Line::from(item)
                }
            })
            .collect();
        menu(items, " GLIDE ", Line::from(best), ACCENT).render(menu_area, buffer, &mut self.menu);
        help_line(
            frame,
            " ↑↓: choose · Enter: select · G: display method · X: quit",
        );
    }
}

// ---------------------------------------------------------------------------------------
// Course code
// ---------------------------------------------------------------------------------------

/// Typing the code of a course to fly it.
pub struct Code {
    text: String,
    wrong: bool,
}

impl Code {
    fn new() -> Self {
        Self {
            text: String::new(),
            wrong: false,
        }
    }
}

impl Scene<Shared> for Code {
    fn event(
        &mut self,
        shared: &mut Shared,
        _ctx: &mut Context,
        event: &Event,
    ) -> Transition<Shared> {
        let Event::Key(key) = event else {
            return Transition::None;
        };
        if key.kind != KeyEventKind::Press {
            return Transition::None;
        }
        match key.code {
            KeyCode::Esc => return Transition::Pop,
            KeyCode::Backspace => {
                self.text.pop();
                self.wrong = false;
            }
            KeyCode::Enter => {
                if let Some(seed) = course::parse_code(&self.text) {
                    shared.sounds.play(Sfx::Blip);
                    shared.request = Some(Request::Play(Mode::Course, seed));
                    return Transition::Pop;
                }
                shared.sounds.play(Sfx::Wrong);
                self.wrong = true;
            }
            KeyCode::Char(c) if self.text.len() < CODE_LENGTH => {
                let c = c.to_ascii_uppercase();
                if CODE_LETTERS.contains(c) {
                    self.text.push(c);
                    self.wrong = false;
                }
            }
            _ => {}
        }
        Transition::None
    }

    fn update(&mut self, _shared: &mut Shared, _ctx: &mut Context) -> Transition<Shared> {
        Transition::None
    }

    fn render(&mut self, _shared: &Shared, _ctx: &Context, frame: &mut Frame<'_>) {
        let mut shown = self.text.clone();
        while shown.len() < CODE_LENGTH {
            shown.push('·');
        }
        let mut lines = vec![
            Line::from("Type the code of a course:"),
            Line::from(""),
            Line::from(shown).style(Style::new().fg(ACCENT).bold()),
            Line::from(""),
        ];
        lines.push(if self.wrong {
            Line::from("Not a code: six of A–Z (no I, O) and 2–9")
                .style(Style::new().fg(Color::rgb(255, 120, 100)))
        } else {
            Line::from("Enter: fly · Esc: back").style(Style::new().dim())
        });
        popup(frame, "Course code", lines, COOL);
    }

    fn is_overlay(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------------------
// Pause
// ---------------------------------------------------------------------------------------

pub struct Pause {
    menu: ListState,
}

impl Pause {
    pub fn new() -> Self {
        Self {
            menu: ListState::with_selected(Some(0)),
        }
    }
}

const PAUSE_MENU: [&str; 4] = ["Continue", "Settings", "Restart", "Back to the title"];

impl Scene<Shared> for Pause {
    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        navigate(shared, ctx, &mut self.menu, PAUSE_MENU.len());
        let input = ctx.input();
        if shared.controls.was_pressed(Action::Pause, input) {
            return Transition::Pop;
        }
        if shared.controls.was_pressed(Action::Confirm, input) {
            shared.sounds.play(Sfx::Blip);
            return match self.menu.selected() {
                Some(1) => Transition::push(Settings::new()),
                Some(2) => {
                    shared.request = Some(Request::Play(shared.mode, shared.last));
                    Transition::Pop
                }
                Some(3) => {
                    shared.store(ctx);
                    shared.request = Some(Request::Title);
                    Transition::Pop
                }
                _ => Transition::Pop,
            };
        }
        Transition::None
    }

    fn render(&mut self, _shared: &Shared, _ctx: &Context, frame: &mut Frame<'_>) {
        let area = frame.area().centered(36, PAUSE_MENU.len() as u16 + 4);
        let buffer = frame.buffer_mut();
        Clear.render(area, buffer);
        menu(
            PAUSE_MENU.iter().map(|&s| Line::from(s)).collect(),
            " Paused ",
            Line::from(" P / Esc: continue "),
            COOL,
        )
        .render(area, buffer, &mut self.menu);
    }

    fn is_overlay(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------------------
// Settings
// ---------------------------------------------------------------------------------------

pub struct Settings {
    menu: ListState,
}

impl Settings {
    pub fn new() -> Self {
        Self {
            menu: ListState::with_selected(Some(0)),
        }
    }
}

/// Rows: music, effects, frame rate, display, upscaling, renderer, anti-aliasing,
/// resolution, performance, controls, back.
const SETTINGS_ROWS: usize = 11;

impl Scene<Shared> for Settings {
    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        navigate(shared, ctx, &mut self.menu, SETTINGS_ROWS);
        let input = ctx.input();
        let controls = &shared.controls;
        let direction = i32::from(controls.was_pressed(Action::Right, input))
            - i32::from(controls.was_pressed(Action::Left, input));
        let confirm = controls.was_pressed(Action::Confirm, input);
        let back = controls.was_pressed(Action::Back, input);
        let row = self.menu.selected().unwrap_or(0);
        if back || (confirm && row == SETTINGS_ROWS - 1) {
            shared.store(ctx);
            return Transition::Pop;
        }
        if confirm && row == SETTINGS_ROWS - 2 {
            shared.sounds.play(Sfx::Blip);
            return Transition::push(Controls::new());
        }
        // Enter flips the switches too.
        let switch = [4, 5, 6, 8].contains(&row);
        let direction = if direction == 0 && confirm && switch {
            1
        } else {
            direction
        };
        if direction == 0 {
            return Transition::None;
        }
        match row {
            0 | 1 => {
                let volume = if row == 0 {
                    &mut shared.music_volume
                } else {
                    &mut shared.effects_volume
                };
                *volume = (*volume as i32 + direction * 10).clamp(0, 100) as u32;
                shared
                    .sounds
                    .volumes(shared.music_volume, shared.effects_volume);
            }
            2 => {
                let current = FRAME_RATES
                    .iter()
                    .position(|&rate| rate == ctx.fps_limit())
                    .unwrap_or(1);
                // Every offered rate is valid.
                let _ = ctx.set_fps_limit(FRAME_RATES[step(current, direction, FRAME_RATES.len())]);
            }
            3 if !shared.modes.is_empty() => {
                let modes = &shared.modes;
                let current = modes
                    .iter()
                    .position(|&mode| mode == ctx.graphics_mode())
                    .unwrap_or(0);
                ctx.set_graphics_mode(modes[step(current, direction, modes.len())]);
            }
            4 => shared.smooth = !shared.smooth,
            5 => {
                let mut renderer = shared.renderer.borrow_mut();
                let on = !renderer.using_gpu();
                renderer.set_gpu(on);
            }
            6 => {
                let mut renderer = shared.renderer.borrow_mut();
                let msaa = if renderer.msaa() > 1 { 1 } else { 4 };
                renderer.set_msaa(msaa);
            }
            7 => {
                let current = RESOLUTIONS
                    .iter()
                    .position(|&r| r == shared.resolution)
                    .unwrap_or(0);
                shared.resolution = RESOLUTIONS[step(current, direction, RESOLUTIONS.len())];
            }
            8 => shared.perf = !shared.perf,
            _ => return Transition::None,
        }
        shared.sounds.play(Sfx::Blip);
        Transition::None
    }

    fn render(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let bar = |value: u32| {
            let filled = (value / 10) as usize;
            format!(
                "{}{} {value:>3}%",
                "█".repeat(filled),
                "·".repeat(10 - filled)
            )
        };
        let renderer = shared.renderer.borrow();
        let drawing = if renderer.gpu_available() {
            if renderer.using_gpu() { "GPU" } else { "CPU" }
        } else {
            "CPU (no GPU)"
        };
        let msaa = if !renderer.gpu_available() {
            "GPU only".to_owned()
        } else if renderer.msaa() > 1 {
            format!("{}×", renderer.msaa())
        } else {
            "off".to_owned()
        };
        let switch =
            |on: bool, yes: &str, no: &str| if on { yes.to_owned() } else { no.to_owned() };
        let items = vec![
            Line::from(format!("Music          {}", bar(shared.music_volume))),
            Line::from(format!("Effects        {}", bar(shared.effects_volume))),
            Line::from(match ctx.fps_limit() {
                FpsLimit::Fixed(rate) => format!("Frames         ◀ {rate} fps ▶"),
                FpsLimit::Unlimited => "Frames         ◀ unlimited ▶".to_owned(),
            }),
            Line::from(format!("Display        ◀ {} ▶", ctx.graphics_mode())),
            Line::from(format!(
                "Upscaling      ◀ {} ▶",
                switch(shared.smooth, "engine (smooth)", "sharp pixels")
            )),
            Line::from(format!("Renderer       ◀ {drawing} ▶")),
            Line::from(format!("Anti-aliasing  ◀ {msaa} ▶")),
            Line::from(format!("Resolution     ◀ {}% ▶", shared.resolution)),
            Line::from(format!(
                "Performance    ◀ {} ▶",
                switch(shared.perf, "shown", "hidden")
            )),
            Line::from("Controls ▸"),
            Line::from("Back"),
        ];
        let note = if shared.sounds.available() {
            " ←→: change · Esc: back ".to_owned()
        } else {
            format!(" ←→: change · sound: {} ", shared.sounds.device())
        };
        let area = frame.area().centered(56, SETTINGS_ROWS as u16 + 4);
        let buffer = frame.buffer_mut();
        Clear.render(area, buffer);
        menu(items, " Settings ", Line::from(note), COOL).render(area, buffer, &mut self.menu);
    }

    fn is_overlay(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------------------
// Controls
// ---------------------------------------------------------------------------------------

/// Every action with its keys; Enter waits for a new key.
pub struct Controls {
    menu: ListState,
    /// Waiting for the key of the selected action.
    waiting: bool,
    /// A key was just bound: skip one update so that key does not act at once.
    settle: bool,
}

impl Controls {
    /// Rows: one per action, then "reset all" and "back".
    const ROWS: usize = ACTIONS.len() + 2;

    fn new() -> Self {
        Self {
            menu: ListState::with_selected(Some(0)),
            waiting: false,
            settle: false,
        }
    }
}

/// The keys of `action` as text (`Up / w`).
fn binding_names(controls: &ActionMap<Action>, action: Action) -> String {
    let names: Vec<String> = controls.bindings(action).map(|b| b.to_string()).collect();
    if names.is_empty() {
        "-".to_owned()
    } else {
        names.join(" / ")
    }
}

/// Whether a key of `action` is also used by another action. Esc and Space are left out:
/// pause / back and confirm share them on purpose.
fn has_clash(controls: &ActionMap<Action>, action: Action) -> bool {
    controls.bindings(action).any(|binding| {
        binding != Binding::Key(KeyCode::Esc)
            && binding != Binding::Key(KeyCode::Char(' '))
            && ACTIONS.iter().any(|&(other, _)| {
                other != action && controls.bindings(other).any(|b| b == binding)
            })
    })
}

impl Scene<Shared> for Controls {
    fn event(
        &mut self,
        shared: &mut Shared,
        _ctx: &mut Context,
        event: &Event,
    ) -> Transition<Shared> {
        let Event::Key(key) = event else {
            return Transition::None;
        };
        if !self.waiting || key.kind != KeyEventKind::Press {
            return Transition::None;
        }
        self.waiting = false;
        self.settle = true;
        // Esc cancels, so it cannot be chosen here.
        if key.code == KeyCode::Esc {
            return Transition::None;
        }
        let code = match key.code {
            KeyCode::Char(c) => KeyCode::Char(c.to_ascii_lowercase()),
            other => other,
        };
        if let Some(&(action, _)) = self.menu.selected().and_then(|row| ACTIONS.get(row)) {
            shared.controls.rebind(action, code);
            shared.sounds.play(Sfx::Blip);
        }
        Transition::None
    }

    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        if self.waiting || std::mem::take(&mut self.settle) {
            return Transition::None;
        }
        navigate(shared, ctx, &mut self.menu, Self::ROWS);
        let input = ctx.input();
        let row = self.menu.selected().unwrap_or(0);
        let confirm = shared.controls.was_pressed(Action::Confirm, input);
        if input.was_pressed(KeyCode::Backspace) {
            if let Some(&(action, _)) = ACTIONS.get(row) {
                shared.controls.unbind(action);
                for binding in default_controls().bindings(action) {
                    shared.controls.add(action, binding);
                }
                shared.sounds.play(Sfx::Blip);
            }
            return Transition::None;
        }
        if shared.controls.was_pressed(Action::Back, input) || (confirm && row == Self::ROWS - 1) {
            shared.store(ctx);
            return Transition::Pop;
        }
        if confirm {
            shared.sounds.play(Sfx::Blip);
            if row < ACTIONS.len() {
                self.waiting = true;
            } else {
                shared.controls = default_controls();
            }
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let warning = Style::new().fg(Color::rgb(255, 120, 100));
        let mut items: Vec<Line<'_>> = ACTIONS
            .iter()
            .enumerate()
            .map(|(row, &(action, name))| {
                let keys = if self.waiting && self.menu.selected() == Some(row) {
                    "press a key… (Esc: cancel)".to_owned()
                } else {
                    binding_names(&shared.controls, action)
                };
                let line = Line::from(format!("{name:<11}{keys}"));
                if has_clash(&shared.controls, action) {
                    line.style(warning)
                } else {
                    line
                }
            })
            .collect();
        items.push(Line::from("Reset all"));
        items.push(Line::from("Back"));
        // On short screens, the list scrolls with the selection.
        let height = (Self::ROWS as u16 + 4).min(ctx.size().height.saturating_sub(2));
        let area = frame.area().centered(54, height);
        let buffer = frame.buffer_mut();
        Clear.render(area, buffer);
        menu(
            items,
            " Controls ",
            Line::from(" Enter: change · Backspace: default "),
            COOL,
        )
        .render(area, buffer, &mut self.menu);
    }

    fn is_overlay(&self) -> bool {
        true
    }
}

// ---------------------------------------------------------------------------------------
// End
// ---------------------------------------------------------------------------------------

pub struct End {
    outcome: Outcome,
    mode: Mode,
    seed: u64,
    record: bool,
    backdrop: Play,
}

impl End {
    pub fn new(outcome: Outcome, mode: Mode, seed: u64, record: bool) -> Self {
        Self {
            outcome,
            mode,
            seed,
            record,
            backdrop: Play::demo(seed),
        }
    }
}

impl Scene<Shared> for End {
    fn start(&mut self, shared: &mut Shared, _ctx: &mut Context) {
        shared.sounds.engine(false);
        shared.sounds.music(Mood::Menu);
    }

    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        self.backdrop.step_backdrop(shared, ctx.dt_secs());
        let input = ctx.input();
        if shared.controls.was_pressed(Action::Confirm, input) {
            return Transition::switch(Title::new(shared));
        }
        if input.was_pressed(KeyCode::Char('r')) {
            return Transition::switch(Play::new(self.mode, self.seed));
        }
        if shared.controls.was_pressed(Action::Quit, input) {
            shared.store(ctx);
            return Transition::Quit;
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, _ctx: &Context, frame: &mut Frame<'_>) {
        self.backdrop.backdrop(shared, frame);
        let code = course::code(self.seed);
        status_line(frame, &format!(" Glide · course {code}"));
        let (title, mut lines) = match self.outcome {
            Outcome::Finished(tenths) => (
                "Course finished",
                vec![Line::from(format!(
                    "Course {code}: all rings in {}.",
                    format_time(tenths)
                ))],
            ),
            Outcome::TimeUp(rings) => (
                "Time is up",
                vec![Line::from(format!("Free flight: {rings} rings."))],
            ),
        };
        if self.record {
            lines.push(Line::from("A new best!").style(Style::new().fg(ACCENT).bold()));
        } else {
            let best = match self.outcome {
                Outcome::Finished(_) => {
                    format!("Best {}", format_time(shared.best_time(self.seed)))
                }
                Outcome::TimeUp(_) => format!("Best {} rings", shared.best_free),
            };
            lines.push(Line::from(best));
        }
        lines.push(Line::from(""));
        lines.push(Line::from("Enter: title · R: fly again · X: quit"));
        popup(frame, title, lines, ACCENT);
    }
}
