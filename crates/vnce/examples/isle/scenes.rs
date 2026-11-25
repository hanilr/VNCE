//! The title (over the island turning slowly), the island code, pause, settings, controls
//! and end scenes.
//!
//! Menus shown over another scene never replace the scene below them (a `Switch` only
//! replaces the top scene): they leave a [`Request`] in the shared data and close, and the
//! scene below acts on it.

use vnce::vnce_core::{Color, Rect, Style};
use vnce::vnce_game::{ActionMap, Binding, Rng, Scene, Transition};
use vnce::vnce_raster::{BlendMode, BlitOptions, Canvas, Font, Rgba};
use vnce::vnce_runtime::{Context, FpsLimit};
use vnce::vnce_term::{Event, Frame, KeyCode, KeyEventKind};
use vnce::vnce_ui::{
    Block, BorderType, Clear, Line, List, ListState, Padding, StatefulWidget, Widget,
};

use crate::audio::{Mood, Sfx};
use crate::island::{self, Island};
use crate::iso::{Layer, Sprites, View, draw_order, draw_tile};
use crate::play::{Outcome, Play};
use crate::visuals::{OCEAN_HEIGHT, OCEAN_WIDTH};
use crate::{
    ACCENT, ACTIONS, Action, COOL, FRAME_RATES, PANEL, Request, Shared, default_controls,
    format_time, help_line, popup, shadow_text, show, status_line,
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

// ---------------------------------------------------------------------------------------
// Backdrop: an island turning slowly over the ocean, in the evening light
// ---------------------------------------------------------------------------------------

struct Backdrop {
    island: Island,
    sprites: Sprites,
    world: Canvas,
    big: Canvas,
}

impl Backdrop {
    fn new(seed: u64) -> Self {
        Self {
            island: Island::generate(seed),
            sprites: Sprites::new(),
            world: Canvas::new(0, 0),
            big: Canvas::new(0, 0),
        }
    }

    /// Draws the island turning at `time` with `title` in big letters over it.
    fn draw(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>, title: &str) {
        let size = ctx.size();
        let area = Rect::new(0, 1, size.width, size.height.saturating_sub(2));
        let (width, height) = frame.image_pixel_size(area);
        if width == 0 || height == 0 {
            return;
        }
        // The whole island (about 1000 × 560 world pixels) fits, in whole pixels.
        let scale = (width as f32 / 1000.0)
            .min(height as f32 / 520.0)
            .floor()
            .max(1.0);
        let view_width = (width as f32 * frame.image_pixel_aspect() / scale) as u32;
        let view_height = (height as f32 / scale) as u32;
        if (self.world.width(), self.world.height()) != (view_width, view_height) {
            self.world = Canvas::new(view_width, view_height);
        }
        let time = ctx.elapsed().as_secs_f32();
        let world = &mut self.world;
        world.set_blend_mode(BlendMode::Replace);
        {
            let mut visuals = shared.visuals.borrow_mut();
            let ocean = visuals.ocean(time);
            for y in (0..view_height).step_by(OCEAN_HEIGHT as usize) {
                for x in (0..view_width).step_by(OCEAN_WIDTH as usize) {
                    world.blit(ocean, x as i32, y as i32, BlitOptions::default());
                }
            }
        }
        world.set_blend_mode(BlendMode::Alpha);
        let view = View::new(
            time * 0.12,
            (view_width as f32 / 2.0, view_height as f32 * 0.58),
        );
        for tile in draw_order(&view, &self.island) {
            draw_tile(world, &view, &self.island, &self.sprites, tile, Layer::Full);
        }
        // Evening light over everything.
        world.set_blend_mode(BlendMode::Multiply);
        world.fill_rect(0, 0, view_width, view_height, Rgba::rgb(255, 200, 170));
        world.set_blend_mode(BlendMode::Alpha);
        let font = Font::builtin();
        let (text_width, text_height) = font.measure(title);
        let scale_text = (view_width * 3 / 5 / text_width.max(1))
            .min(view_height / 5 / text_height.max(1))
            .max(1);
        let x = (view_width.saturating_sub(text_width * scale_text)) / 2;
        let y = view_height / 10;
        shadow_text(
            world,
            x as i32,
            y as i32,
            title,
            Rgba::rgb(255, 214, 140),
            scale_text,
        );
        let subtitle = "A VNCE SHOWCASE IN 2.5D";
        let (sw, _) = font.measure(subtitle);
        shadow_text(
            world,
            (view_width.saturating_sub(sw)) as i32 / 2,
            (y + text_height * scale_text + 4) as i32,
            subtitle,
            Rgba::rgb(210, 230, 255),
            1,
        );
        show(shared, frame, area, &self.world, &mut self.big);
    }
}

// ---------------------------------------------------------------------------------------
// Title
// ---------------------------------------------------------------------------------------

pub struct Title {
    menu: ListState,
    backdrop: Backdrop,
}

impl Title {
    pub fn new(shared: &Shared) -> Self {
        Self {
            menu: ListState::with_selected(Some(0)),
            backdrop: Backdrop::new(shared.last),
        }
    }
}

const TITLE_MENU: [&str; 5] = [
    "New island",
    "Replay last island",
    "Island code…",
    "Settings",
    "Quit",
];

impl Scene<Shared> for Title {
    fn start(&mut self, shared: &mut Shared, ctx: &mut Context) {
        shared.sounds.music(Mood::Day);
        shared.sounds.ambience(true);
        shared.detect_modes(ctx);
    }

    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        if let Some(Request::Play(seed)) = shared.request.take() {
            return Transition::switch(Play::new(seed));
        }
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
                Some(0) => {
                    let seed = Rng::from_time().next_u64() % island::CODES;
                    Transition::switch(Play::new(seed))
                }
                Some(1) => Transition::switch(Play::new(shared.last)),
                Some(2) => Transition::push(Code::new()),
                Some(3) => Transition::push(Settings::new()),
                _ => {
                    shared.store(ctx);
                    Transition::Quit
                }
            };
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        self.backdrop.draw(shared, ctx, frame, "ISLE");
        status_line(
            frame,
            &format!(
                " Isle · {} · ocean {} · sound: {}",
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
        let centered = area.centered(34, TITLE_MENU.len() as u16 + 4);
        let menu_area = Rect {
            y: (centered.y + area.height / 6).min(area.height.saturating_sub(centered.height + 1)),
            ..centered
        };
        let buffer = frame.buffer_mut();
        Clear.render(menu_area, buffer);
        let items = TITLE_MENU
            .iter()
            .enumerate()
            .map(|(i, &item)| {
                if i == 1 {
                    Line::from(format!("Replay island {}", island::code(shared.last)))
                } else {
                    Line::from(item)
                }
            })
            .collect();
        menu(items, " ISLE ", Line::from(best), ACCENT).render(menu_area, buffer, &mut self.menu);
        help_line(
            frame,
            " ↑↓: choose · Enter: select · G: display method · X: quit",
        );
    }
}

// ---------------------------------------------------------------------------------------
// Island code
// ---------------------------------------------------------------------------------------

/// Typing the code of an island to play it.
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
                if let Some(seed) = island::parse_code(&self.text) {
                    shared.sounds.play(Sfx::Blip);
                    shared.request = Some(Request::Play(seed));
                    return Transition::Pop;
                }
                shared.sounds.play(Sfx::Wrong);
                self.wrong = true;
            }
            KeyCode::Char(c) if self.text.len() < island::CODE_LENGTH => {
                let c = c.to_ascii_uppercase();
                if island::CODE_LETTERS.contains(c) {
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
        while shown.len() < island::CODE_LENGTH {
            shown.push('·');
        }
        let mut lines = vec![
            Line::from("Type the code of an island:"),
            Line::from(""),
            Line::from(shown).style(Style::new().fg(ACCENT).bold()),
            Line::from(""),
        ];
        lines.push(if self.wrong {
            Line::from("Not a code: six of A–Z (no I, O) and 2–9")
                .style(Style::new().fg(Color::rgb(255, 120, 100)))
        } else {
            Line::from("Enter: play · Esc: back").style(Style::new().dim())
        });
        popup(frame, "Island code", lines, COOL);
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

const PAUSE_MENU: [&str; 4] = [
    "Continue",
    "Settings",
    "Restart this island",
    "Back to the title",
];

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
                    shared.request = Some(Request::Play(shared.last));
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

/// Rows: music, effects, frame rate, display, upscaling, ocean, minimap, performance,
/// controls, back.
const SETTINGS_ROWS: usize = 10;

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
        let direction = if direction == 0 && confirm && (4..=7).contains(&row) {
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
                let mut visuals = shared.visuals.borrow_mut();
                let on = !visuals.using_gpu();
                visuals.set_gpu(on);
            }
            6 => shared.map = !shared.map,
            7 => shared.perf = !shared.perf,
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
        let ocean = if visuals.gpu_available() {
            if visuals.using_gpu() { "GPU" } else { "CPU" }
        } else {
            "CPU (no GPU)"
        };
        let switch =
            |on: bool, yes: &str, no: &str| if on { yes.to_owned() } else { no.to_owned() };
        let items = vec![
            Line::from(format!("Music        {}", bar(shared.music_volume))),
            Line::from(format!("Effects      {}", bar(shared.effects_volume))),
            Line::from(match ctx.fps_limit() {
                FpsLimit::Fixed(rate) => format!("Frames       ◀ {rate} fps ▶"),
                FpsLimit::Unlimited => "Frames       ◀ unlimited ▶".to_owned(),
            }),
            Line::from(format!("Display      ◀ {} ▶", ctx.graphics_mode())),
            Line::from(format!(
                "Upscaling    ◀ {} ▶",
                switch(shared.smooth, "engine (smooth)", "sharp pixels")
            )),
            Line::from(format!("Ocean        ◀ {ocean} ▶")),
            Line::from(format!(
                "Minimap      ◀ {} ▶",
                switch(shared.map, "shown", "hidden")
            )),
            Line::from(format!(
                "Performance  ◀ {} ▶",
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
        let area = frame.area().centered(54, SETTINGS_ROWS as u16 + 4);
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
    seed: u64,
    record: bool,
    backdrop: Backdrop,
}

impl End {
    pub fn new(outcome: Outcome, seed: u64, record: bool) -> Self {
        Self {
            outcome,
            seed,
            record,
            backdrop: Backdrop::new(seed),
        }
    }
}

impl Scene<Shared> for End {
    fn start(&mut self, shared: &mut Shared, _ctx: &mut Context) {
        shared.sounds.music(match self.outcome {
            Outcome::Won(_) => Mood::Day,
            Outcome::Lost(_) => Mood::Night,
        });
    }

    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        let input = ctx.input();
        if shared.controls.was_pressed(Action::Confirm, input) {
            return Transition::switch(Title::new(shared));
        }
        if input.was_pressed(KeyCode::Char('r')) {
            return Transition::switch(Play::new(self.seed));
        }
        if shared.controls.was_pressed(Action::Quit, input) {
            shared.store(ctx);
            return Transition::Quit;
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let won = matches!(self.outcome, Outcome::Won(_));
        self.backdrop
            .draw(shared, ctx, frame, if won { "LIT" } else { "DARK" });
        status_line(
            frame,
            &format!(" Isle · island {}", island::code(self.seed)),
        );
        let mut lines = vec![match self.outcome {
            Outcome::Won(time) => Line::from(format!(
                "The beacon shines over island {} after {}.",
                island::code(self.seed),
                format_time(time)
            )),
            Outcome::Lost(crystals) => Line::from(format!(
                "Night fell with {crystals} of {} crystals.",
                island::CRYSTALS
            )),
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
        lines.push(Line::from("Enter: title · R: this island again · X: quit"));
        let accent = if won {
            Color::rgb(255, 220, 130)
        } else {
            Color::rgb(140, 160, 255)
        };
        popup(
            frame,
            if won { "Beacon lit" } else { "Night fell" },
            lines,
            accent,
        );
    }
}
