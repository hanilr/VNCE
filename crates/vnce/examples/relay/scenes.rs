//! The title, pause, settings, controls and end scenes.

use std::time::Instant;

use vnce::vnce_core::{Color, Style};
use vnce::vnce_game::{ActionMap, Binding, Rng, Scene, Transition};
use vnce::vnce_raster::{Canvas, Font, Rgba};
use vnce::vnce_runtime::{Context, FpsLimit};
use vnce::vnce_term::{Event, Frame, KeyCode, KeyEventKind};
use vnce::vnce_ui::{
    Block, BorderType, Clear, Line, List, ListState, Padding, StatefulWidget, Widget,
};

use crate::audio::Sfx;
use crate::play::Play;
use crate::visuals::Slot;
use crate::{
    ACCENT, ACTIONS, Action, COOL, FRAME_RATES, PANEL, Shared, default_controls, format_time,
    help_line, popup, present, shadow_text, status_line, view,
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

/// Canvases a backdrop scene keeps between frames: the picture with the title, and its
/// enlargement to the screen.
struct Buffers {
    canvas: Canvas,
    big: Canvas,
}

impl Buffers {
    fn new() -> Self {
        Self {
            canvas: Canvas::new(0, 0),
            big: Canvas::new(0, 0),
        }
    }
}

/// The nebula over the whole view with the big title on it.
fn backdrop(
    shared: &Shared,
    ctx: &Context,
    frame: &mut Frame<'_>,
    buffers: &mut Buffers,
    title: &str,
) {
    let (area, width, height) = view(ctx, frame);
    if width == 0 {
        return;
    }
    let time = ctx.elapsed().as_secs_f32();
    let canvas = &mut buffers.canvas;
    // A copy into the kept canvas: the title is drawn on it, and nothing is allocated.
    canvas.clone_from(
        shared
            .visuals
            .borrow_mut()
            .nebula(Slot::Backdrop, width, height, time),
    );
    let font = Font::builtin();
    let (text_width, text_height) = font.measure(title);
    let scale = (width * 3 / 5 / text_width)
        .min(height / 5 / text_height)
        .max(1);
    let x = (width - (text_width * scale).min(width)) / 2;
    let y = height / 10;
    shadow_text(
        canvas,
        x as i32,
        y as i32,
        title,
        Rgba::rgb(255, 210, 110),
        scale,
    );
    let subtitle = "A VNCE SHOWCASE";
    let (sw, _) = font.measure(subtitle);
    shadow_text(
        canvas,
        (width.saturating_sub(sw)) as i32 / 2,
        (y + text_height * scale + 4) as i32,
        subtitle,
        Rgba::rgb(200, 220, 255),
        1,
    );
    present(frame, area, &buffers.canvas, &mut buffers.big);
}

// ---------------------------------------------------------------------------------------
// Title
// ---------------------------------------------------------------------------------------

pub struct Title {
    menu: ListState,
    buffers: Buffers,
}

impl Title {
    pub fn new() -> Self {
        Self {
            menu: ListState::with_selected(Some(0)),
            buffers: Buffers::new(),
        }
    }
}

const TITLE_MENU: [&str; 3] = ["Start", "Settings", "Quit"];

impl Scene<Shared> for Title {
    fn start(&mut self, shared: &mut Shared, ctx: &mut Context) {
        shared.sounds.music(false);
        shared.detect_modes(ctx);
    }

    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
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
                Some(0) => Transition::switch(Play::new(Rng::from_time().next_u64())),
                Some(1) => Transition::push(Settings::new()),
                _ => {
                    shared.store(ctx);
                    Transition::Quit
                }
            };
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let started = Instant::now();
        backdrop(shared, ctx, frame, &mut self.buffers, "RELAY");
        status_line(
            frame,
            &format!(
                " Relay · {} · {} · sound: {}",
                frame.graphics_mode(),
                shared.visuals.borrow().describe(),
                shared.sounds.device()
            ),
        );
        let best = if shared.best > 0 {
            format!(" best {} ", format_time(shared.best))
        } else {
            String::new()
        };
        let area = frame.area();
        let centered = area.centered(30, 9);
        let menu_area = vnce::vnce_core::Rect {
            y: (centered.y + area.height / 6).min(area.height.saturating_sub(centered.height + 1)),
            ..centered
        };
        let buffer = frame.buffer_mut();
        Clear.render(menu_area, buffer);
        menu(
            TITLE_MENU.iter().map(|&s| Line::from(s)).collect(),
            " RELAY ",
            Line::from(best),
            ACCENT,
        )
        .render(menu_area, buffer, &mut self.menu);
        help_line(
            frame,
            " ↑↓: choose · Enter: select · G: display method · Q: quit",
        );
        shared
            .stats
            .borrow_mut()
            .record(started.elapsed(), ctx.fps());
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

const PAUSE_MENU: [&str; 3] = ["Continue", "Settings", "Back to the title"];

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
                    shared.sounds.music(false);
                    shared.store(ctx);
                    // A switch here would only replace this menu and leave the station
                    // below it: the station switches to the title when it sees the request.
                    shared.to_title = true;
                    Transition::Pop
                }
                _ => Transition::Pop,
            };
        }
        Transition::None
    }

    fn render(&mut self, _shared: &Shared, _ctx: &Context, frame: &mut Frame<'_>) {
        let area = frame.area().centered(34, 9);
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

/// Rows: music, effects, frame rate, display, pictures, performance, controls, back.
const SETTINGS_ROWS: usize = 8;

/// Moves `index` by `direction` through `length` items, wrapping around.
fn step(index: usize, direction: i32, length: usize) -> usize {
    (index as i32 + direction).rem_euclid(length as i32) as usize
}

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
        if confirm && row == 6 {
            shared.sounds.play(Sfx::Blip);
            return Transition::push(Controls::new());
        }
        // Enter flips the switches too.
        let direction = if direction == 0 && confirm && (row == 4 || row == 5) {
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
            4 => {
                let mut visuals = shared.visuals.borrow_mut();
                let on = !visuals.using_gpu();
                visuals.set_gpu(on);
            }
            5 => shared.perf = !shared.perf,
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
        let visuals = shared.visuals.borrow();
        let pictures = if visuals.gpu_available() {
            if visuals.using_gpu() { "GPU" } else { "CPU" }
        } else {
            "CPU (no GPU)"
        };
        let items = vec![
            Line::from(format!("Music        {}", bar(shared.music_volume))),
            Line::from(format!("Effects      {}", bar(shared.effects_volume))),
            Line::from(match ctx.fps_limit() {
                FpsLimit::Fixed(rate) => format!("Frames       ◀ {rate} fps ▶"),
                FpsLimit::Unlimited => "Frames       ◀ unlimited ▶".to_owned(),
            }),
            Line::from(format!("Display      ◀ {} ▶", ctx.graphics_mode())),
            Line::from(format!("Pictures     ◀ {pictures} ▶")),
            Line::from(format!(
                "Performance  ◀ {} ▶",
                if shared.perf { "shown" } else { "hidden" }
            )),
            Line::from("Controls ▸"),
            Line::from("Back"),
        ];
        let note = if shared.sounds.available() {
            " ←→: change · Esc: back ".to_owned()
        } else {
            format!(" ←→: change · sound: {} ", shared.sounds.device())
        };
        let area = frame.area().centered(52, SETTINGS_ROWS as u16 + 4);
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
/// pause / back and confirm / use share them on purpose.
fn has_clash(controls: &ActionMap<Action>, action: Action) -> bool {
    controls.bindings(action).any(|binding| {
        binding != Binding::Key(KeyCode::Esc)
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

    fn render(&mut self, shared: &Shared, _ctx: &Context, frame: &mut Frame<'_>) {
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
                let line = Line::from(format!("{name:<9}{keys}"));
                if has_clash(&shared.controls, action) {
                    line.style(warning)
                } else {
                    line
                }
            })
            .collect();
        items.push(Line::from("Reset all"));
        items.push(Line::from("Back"));
        let area = frame.area().centered(52, Self::ROWS as u16 + 4);
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
    won: bool,
    /// Play time in tenths of a second.
    time: u32,
    cores: u32,
    record: bool,
    buffers: Buffers,
}

impl End {
    pub fn won(time: u32, record: bool) -> Self {
        Self {
            won: true,
            time,
            cores: 5,
            record,
            buffers: Buffers::new(),
        }
    }

    pub fn lost(cores: u32, time: u32) -> Self {
        Self {
            won: false,
            time,
            cores,
            record: false,
            buffers: Buffers::new(),
        }
    }
}

impl Scene<Shared> for End {
    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        let input = ctx.input();
        if shared.controls.was_pressed(Action::Confirm, input) {
            return Transition::switch(Title::new());
        }
        if shared.controls.was_pressed(Action::Quit, input) {
            shared.store(ctx);
            return Transition::Quit;
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let title = if self.won { "ONLINE" } else { "OFFLINE" };
        backdrop(shared, ctx, frame, &mut self.buffers, title);
        status_line(frame, " Relay");
        let mut lines = vec![if self.won {
            Line::from(format!(
                "The relay is back online in {}.",
                format_time(self.time)
            ))
        } else {
            Line::from(format!(
                "Your suit ran out of energy with {} of 5 cores.",
                self.cores
            ))
        }];
        if self.record {
            lines.push(Line::from("A new best time!").style(Style::new().fg(ACCENT).bold()));
        } else if shared.best > 0 {
            lines.push(Line::from(format!(
                "Best time {}",
                format_time(shared.best)
            )));
        }
        lines.push(Line::from(""));
        lines.push(Line::from("Enter: title · Q: quit"));
        let accent = if self.won {
            Color::rgb(120, 255, 170)
        } else {
            Color::rgb(255, 120, 100)
        };
        popup(
            frame,
            if self.won {
                "Relay online"
            } else {
                "Relay offline"
            },
            lines,
            accent,
        );
    }
}
