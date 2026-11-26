//! A dashboard built with the widgets of `vnce_ui`: a menu, gauges, a live pixel chart,
//! wrapped scrolling text, a popup and an FPS overlay. Two more pages, picked with the
//! menu or the tabs, show the controls (a text field, a slider, a checkbox, a button) and
//! a table; everything also works with the mouse.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example ui
//! ```
//!
//! `↑` / `↓` (or `W` / `S`): move in the menu · `Enter`: choose · `P`: popup ·
//! `F`: FPS overlay · `Esc`: close the popup or quit · `Q`: quit.
//!
//! On the controls and scores pages `Tab` moves the focus to the next control; in the
//! name field every key types, and `Esc`, `Enter` or `Tab` leave it. `←` / `→` change the
//! volume, `Space` or `Enter` toggle the checkbox and press the button.

// Demo code converts freely between sizes, times and floats; every value stays far inside
// the target ranges.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    reason = "sizes and times are small"
)]

use std::collections::VecDeque;
use std::time::Instant;

use vnce::vnce_core::{Buffer, Color, Rect, Style};
use vnce::vnce_raster::{Canvas, PixelMode, RenderOptions, Rgba};
use vnce::vnce_runtime::{Context, Game, RunConfig};
use vnce::vnce_term::input::MouseButton;
use vnce::vnce_term::{Event, Frame, KeyCode, KeyEventKind};
use vnce::vnce_ui::{
    Alignment, Block, BorderType, Button, Checkbox, Clear, Constraint, FpsOverlay, Gauge, Layout,
    Line, List, ListState, Padding, Paragraph, Slider, Span, StatefulWidget, Table, TableState,
    Tabs, TextInput, TextInputState, Widget,
};

const MENU: [&str; 6] = [
    "Dashboard",
    "Controls",
    "High scores",
    "Show a popup",
    "Toggle FPS overlay",
    "Quit",
];

/// The pages, in the order of the tabs (also the first three menu rows).
const PAGES: [&str; 3] = ["Dashboard", "Controls", "High scores"];

const SCORES: [[&str; 3]; 8] = [
    ["Ada", "12 400", "Isle"],
    ["Grace", "11 950", "Glide"],
    ["Linus", "10 020", "Relay"],
    ["Margaret", "9 870", "Isle"],
    ["Ken", "8 310", "Glide"],
    ["Barbara", "7 760", "Relay"],
    ["Dennis", "6 500", "Isle"],
    ["Frances", "5 120", "Glide"],
];

const STORY: &str = "Variation draws everything into one buffer of cells: widgets, pixel \
graphics, GPU frames and images can share a frame. Widgets are immediate mode: they are \
built and drawn again on every frame, and only the cells that changed reach the terminal. \
Text wraps at word boundaries, keeps wide characters such as 日本語 whole, and lines that \
do not fit end with an ellipsis.";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut demo = Demo::new();
    let mut config = RunConfig::default();
    config.terminal.mouse_capture = true;
    vnce::run(&mut demo, config)?;
    Ok(())
}

/// What the keyboard works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Focus {
    Menu,
    Name,
    Volume,
    Sound,
    Save,
    Scores,
}

/// Where the clickable things were drawn in the last frame.
#[derive(Debug, Clone, Copy, Default)]
struct Areas {
    tabs: Rect,
    menu: Rect,
    name: Rect,
    volume: Rect,
    sound: Rect,
    save: Rect,
    scores: Rect,
}

struct Demo {
    menu: ListState,
    popup: bool,
    overlay: bool,
    /// Pixel mode for the chart.
    mode: PixelMode,
    last_frame: Option<Instant>,
    /// Recent frame times in milliseconds, oldest first.
    frame_times: VecDeque<f32>,
    /// The page shown (an index into `PAGES`).
    page: usize,
    focus: Focus,
    name: TextInputState,
    /// Volume in percent.
    volume: f64,
    sound: bool,
    /// What the save button saved, shown under it.
    saved: Option<String>,
    scores: TableState,
    areas: Areas,
}

impl Demo {
    fn new() -> Self {
        let mut name = TextInputState::new().max_len(16);
        name.set_text("Player");
        Self {
            menu: ListState::with_selected(Some(0)),
            popup: false,
            overlay: true,
            mode: PixelMode::HalfBlock,
            last_frame: None,
            frame_times: VecDeque::new(),
            page: 0,
            focus: Focus::Menu,
            name,
            volume: 70.0,
            sound: true,
            saved: None,
            scores: TableState::with_selected(Some(0)),
            areas: Areas::default(),
        }
    }

    /// The controls `Tab` cycles through on the page shown.
    fn focus_order(&self) -> &'static [Focus] {
        match self.page {
            1 => &[
                Focus::Menu,
                Focus::Name,
                Focus::Volume,
                Focus::Sound,
                Focus::Save,
            ],
            2 => &[Focus::Menu, Focus::Scores],
            _ => &[Focus::Menu],
        }
    }

    fn next_focus(&mut self) {
        let order = self.focus_order();
        let at = order.iter().position(|&f| f == self.focus).unwrap_or(0);
        self.focus = order[(at + 1) % order.len()];
    }

    fn show_page(&mut self, page: usize) {
        self.page = page;
        self.focus = Focus::Menu;
        self.menu.select(Some(page));
    }

    fn change_volume(&mut self, step: f64) {
        self.volume = (self.volume + step).clamp(0.0, 100.0);
    }

    fn save(&mut self) {
        self.saved = Some(format!(
            "Saved: {}, volume {:.0}%, sound {}.",
            self.name.text(),
            self.volume,
            if self.sound { "on" } else { "off" }
        ));
    }

    /// Does what menu row `row` says.
    fn choose(&mut self, ctx: &mut Context, row: usize) {
        match row {
            0..=2 => self.show_page(row),
            3 => self.popup = true,
            4 => self.overlay = !self.overlay,
            _ => ctx.quit(),
        }
    }

    /// Clicks and drags: tabs, menu rows, the controls and table rows.
    fn mouse(&mut self, ctx: &mut Context) {
        let input = ctx.input();
        let Some(at) = input.mouse_position() else {
            return;
        };
        let pressed = input.was_mouse_pressed(MouseButton::Left);
        let held = input.is_mouse_down(MouseButton::Left);
        let areas = self.areas;
        if held && self.page == 1 && self.focus == Focus::Volume && areas.volume.contains(at) {
            // Dragging the thumb.
            self.volume = volume_slider(self.volume).value_at(areas.volume, at.x);
        }
        if !pressed || self.popup {
            return;
        }
        let row = self.page == 2;
        let controls = self.page == 1;
        if let Some(page) = tabs(self.page).tab_at(areas.tabs, at) {
            self.show_page(page);
        } else if let Some(item) = menu().item_at(areas.menu, &self.menu, at) {
            self.menu.select(Some(item));
            self.focus = Focus::Menu;
            self.choose(ctx, item);
        } else if controls && areas.name.contains(at) {
            self.focus = Focus::Name;
        } else if controls && areas.volume.contains(at) {
            self.focus = Focus::Volume;
            self.volume = volume_slider(self.volume).value_at(areas.volume, at.x);
        } else if controls && areas.sound.contains(at) {
            self.focus = Focus::Sound;
            self.sound = !self.sound;
        } else if controls && Button::contains(areas.save, at) {
            self.focus = Focus::Save;
            self.save();
        } else if row {
            if let Some(index) = scores_table().row_at(areas.scores, &self.scores, at) {
                self.focus = Focus::Scores;
                self.scores.select(Some(index));
            }
        }
    }
}

fn menu() -> List<'static> {
    List::new(MENU)
        .highlight_symbol("▶ ")
        .highlight_style(
            Style::new()
                .fg(Color::rgb(20, 20, 30))
                .bg(Color::rgb(240, 200, 90)),
        )
        .block(
            Block::bordered()
                .border_type(BorderType::Rounded)
                .title(" Menu ")
                .padding(Padding::horizontal(1)),
        )
}

fn tabs(page: usize) -> Tabs<'static> {
    Tabs::new(PAGES).select(page).highlight_style(
        Style::new()
            .fg(Color::rgb(20, 20, 30))
            .bg(Color::rgb(170, 150, 240)),
    )
}

fn volume_slider(volume: f64) -> Slider<'static> {
    Slider::new(volume, 0.0, 100.0)
        .show_value(true)
        .filled_style(Style::new().fg(Color::rgb(90, 200, 120)))
        .block(Block::bordered().title(" Volume (←→ or drag) "))
}

fn scores_table() -> Table<'static> {
    Table::new(
        SCORES,
        [
            Constraint::Length(10),
            Constraint::Length(8),
            Constraint::Fill(1),
        ],
    )
    .header(["Name", "Score", "Game"])
    .highlight_symbol("▶ ")
    .block(Block::bordered().title(" High scores (click a row) "))
}

impl Game for Demo {
    fn init(&mut self, ctx: &mut Context) {
        self.mode = PixelMode::detect(ctx.capabilities().color_depth);
    }

    fn event(&mut self, ctx: &mut Context, event: &Event) {
        // The name field takes every key but the ones that leave it.
        if self.focus == Focus::Name && !self.popup {
            let leave = matches!(
                event,
                Event::Key(key) if key.kind == KeyEventKind::Press
                    && matches!(key.code, KeyCode::Esc | KeyCode::Enter | KeyCode::Tab)
            );
            if leave {
                self.next_focus();
            } else {
                self.name.handle_event(event);
            }
            return;
        }
        let Event::Key(key) = event else {
            return;
        };
        if key.kind != KeyEventKind::Press {
            return;
        }
        match (key.code, self.focus) {
            (KeyCode::Esc, _) if self.popup => self.popup = false,
            (KeyCode::Esc | KeyCode::Char('q' | 'Q'), _) => ctx.quit(),
            (KeyCode::Tab, _) => self.next_focus(),
            (KeyCode::Up | KeyCode::Char('w' | 'W'), Focus::Scores) => {
                self.scores.select_previous(SCORES.len(), false);
            }
            (KeyCode::Down | KeyCode::Char('s' | 'S'), Focus::Scores) => {
                self.scores.select_next(SCORES.len(), false);
            }
            (KeyCode::Up | KeyCode::Char('w' | 'W'), _) => {
                self.focus = Focus::Menu;
                self.menu.select_previous(MENU.len(), true);
            }
            (KeyCode::Down | KeyCode::Char('s' | 'S'), _) => {
                self.focus = Focus::Menu;
                self.menu.select_next(MENU.len(), true);
            }
            (KeyCode::Left, Focus::Volume) => self.change_volume(-5.0),
            (KeyCode::Right, Focus::Volume) => self.change_volume(5.0),
            (KeyCode::Char(' ') | KeyCode::Enter, Focus::Sound) => self.sound = !self.sound,
            (KeyCode::Char(' ') | KeyCode::Enter, Focus::Save) => self.save(),
            (KeyCode::Char('p' | 'P'), _) => self.popup = !self.popup,
            (KeyCode::Char('f' | 'F'), _) => self.overlay = !self.overlay,
            (KeyCode::Enter, _) => {
                if let Some(row) = self.menu.selected() {
                    self.choose(ctx, row);
                }
            }
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut Context) {
        self.mouse(ctx);
    }

    fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>) {
        let now = Instant::now();
        if let Some(last) = self.last_frame.replace(now) {
            self.frame_times
                .push_back((now - last).as_secs_f32() * 1000.0);
            if self.frame_times.len() > 24 {
                self.frame_times.pop_front();
            }
        }
        let area = frame.area();
        let buffer = frame.buffer_mut();

        let [title, body, help] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Fill(1),
            Constraint::Length(1),
        ])
        .areas(area);
        Paragraph::new(Line::from(vec![
            Span::styled(" Variation ", Style::new().bold()),
            Span::raw("UI widgets"),
        ]))
        .alignment(Alignment::Center)
        .style(
            Style::new()
                .fg(Color::rgb(20, 20, 30))
                .bg(Color::rgb(170, 150, 240)),
        )
        .render(title, buffer);
        let help_text = if self.focus == Focus::Name {
            " Typing the name · Esc / Enter / Tab: leave the field"
        } else {
            " ↑↓: menu · Enter: choose · Tab: next control · P: popup · F: FPS · Esc / Q: quit"
        };
        Paragraph::new(help_text)
            .style(Style::new().dim())
            .render(help, buffer);

        let [menu_area, main] = Layout::horizontal([Constraint::Length(24), Constraint::Fill(1)])
            .spacing(1)
            .areas(body);
        menu().render(menu_area, buffer, &mut self.menu);
        self.areas.menu = menu_area;

        let [tabs_area, main] =
            Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).areas(main);
        tabs(self.page).render(tabs_area, buffer);
        self.areas.tabs = tabs_area;

        let mut chart = Rect::default();
        match self.page {
            1 => self.render_controls(main, buffer),
            2 => {
                let highlight = if self.focus == Focus::Scores {
                    Style::new().reverse()
                } else {
                    Style::new().bold()
                };
                scores_table()
                    .highlight_style(highlight)
                    .render(main, buffer, &mut self.scores);
                self.areas.scores = main;
            }
            _ => chart = render_dashboard(main, buffer, ctx, self.mode),
        }

        if self.overlay && !chart.is_empty() {
            FpsOverlay::new(ctx.fps())
                .ups(ctx.ups())
                .history(self.frame_times.iter().copied())
                .style(Style::new().bg(Color::rgb(30, 30, 45)))
                .render(chart.inner(1, 1), buffer);
        }
        if self.popup {
            let popup = area.centered(44, 7);
            Clear.render(popup, buffer);
            Paragraph::new(vec![
                Line::from("A popup drawn over everything else."),
                Line::from(""),
                Line::from("Esc or P closes it.").centered(),
            ])
            .wrap(true)
            .style(Style::new().bg(Color::rgb(25, 30, 60)))
            .block(
                Block::bordered()
                    .border_type(BorderType::Double)
                    .title(Line::from(" Popup ").centered())
                    .border_style(Style::new().fg(Color::rgb(120, 180, 255)))
                    .padding(Padding::new(2, 2, 1, 0)),
            )
            .render(popup, buffer);
        }
    }
}

impl Demo {
    /// The controls page: a text field, a slider, a checkbox and a button.
    fn render_controls(&mut self, area: Rect, buffer: &mut Buffer) {
        let [name, volume, sound, save, note] = Layout::vertical([
            Constraint::Length(3),
            Constraint::Length(3),
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Fill(1),
        ])
        .spacing(1)
        .areas(area.inner(1, 1));
        let narrow = |r: Rect, width: u16| Rect::new(r.x, r.y, r.width.min(width), r.height);
        let (name, volume, save) = (narrow(name, 44), narrow(volume, 44), narrow(save, 12));
        let focus = self.focus;
        let border = |of: Focus| {
            if focus == of {
                Style::new().fg(Color::rgb(240, 200, 90))
            } else {
                Style::new()
            }
        };
        TextInput::new()
            .placeholder("Your name")
            .focused(focus == Focus::Name)
            .block(
                Block::bordered()
                    .title(" Name (click or Tab, then type) ")
                    .border_style(border(Focus::Name)),
            )
            .render(name, buffer, &mut self.name);
        volume_slider(self.volume)
            .block(
                Block::bordered()
                    .title(" Volume (←→ or drag) ")
                    .border_style(border(Focus::Volume)),
            )
            .render(volume, buffer);
        Checkbox::new("Sound effects (Space or click)", self.sound)
            .style(border(Focus::Sound))
            .render(sound, buffer);
        Button::new("Save")
            .focused(focus == Focus::Save)
            .block(Block::bordered().border_type(BorderType::Rounded))
            .render(save, buffer);
        if let Some(saved) = &self.saved {
            Paragraph::new(saved.as_str())
                .style(Style::new().dim())
                .wrap(true)
                .render(note, buffer);
        }
        self.areas.name = name;
        self.areas.volume = volume;
        self.areas.sound = sound;
        self.areas.save = save;
    }
}

/// The first page: gauges, a live chart and wrapped text. Returns the chart's area (for
/// the FPS overlay).
fn render_dashboard(main: Rect, buffer: &mut Buffer, ctx: &Context, mode: PixelMode) -> Rect {
    let t = ctx.elapsed().as_secs_f32();
    let [gauges, chart_area, text] = Layout::vertical([
        Constraint::Length(5),
        Constraint::Fill(1),
        Constraint::Length(6),
    ])
    .areas(main);
    render_gauges(gauges, buffer, t);
    render_chart(chart_area, buffer, t, mode);
    let paragraph = Paragraph::new(STORY).wrap(true);
    let block = Block::bordered()
        .title(" Wrapped text ")
        .title_bottom(Line::from(" scrolls by itself ").right_aligned())
        .padding(Padding::horizontal(1));
    let inner = block.inner(text);
    let rows = paragraph.line_count(inner.width);
    let hidden = rows.saturating_sub(usize::from(inner.height));
    let scroll = if hidden == 0 {
        0
    } else {
        (ctx.elapsed().as_secs() as usize / 2) % (hidden + 1)
    };
    paragraph
        .scroll(scroll as u16)
        .block(block)
        .render(text, buffer);
    chart_area
}

/// Three animated gauges in a block.
fn render_gauges(area: Rect, buffer: &mut Buffer, t: f32) {
    let block = Block::bordered().title(" Gauges ");
    let inner = block.inner(area);
    block.render(area, buffer);
    let rows = Layout::vertical([Constraint::Length(1); 3]).split(inner);
    let gauges = [
        (
            "Health",
            0.5 + 0.45 * (t * 0.8).sin(),
            Color::rgb(220, 70, 70),
        ),
        (
            "Mana",
            0.5 + 0.45 * (t * 1.3 + 1.0).sin(),
            Color::rgb(80, 140, 240),
        ),
        ("Loading", (t * 0.1).fract(), Color::rgb(90, 200, 120)),
    ];
    for (row, (name, value, color)) in rows.into_iter().zip(gauges) {
        let [label, bar] =
            Layout::horizontal([Constraint::Length(9), Constraint::Fill(1)]).areas(row);
        Paragraph::new(name).render(label, buffer);
        Gauge::new()
            .ratio(f64::from(value))
            .gauge_style(Style::new().fg(color).bg(Color::rgb(40, 40, 50)))
            .render(bar, buffer);
    }
}

/// A live chart drawn with pixels inside a block.
fn render_chart(area: Rect, buffer: &mut Buffer, t: f32, mode: PixelMode) {
    let block = Block::bordered().title(" Pixel chart ");
    let inner = block.inner(area);
    block.render(area, buffer);
    if inner.is_empty() {
        return;
    }
    let mut canvas = Canvas::for_area(inner, mode);
    canvas.clear(Rgba::rgb(14, 16, 26));
    let (width, height) = (canvas.width() as f32, canvas.height() as f32);
    for (offset, speed, color) in [
        (0.0, 1.0, Rgba::rgb(90, 200, 255)),
        (2.0, 1.7, Rgba::rgb(255, 150, 90)),
    ] {
        let y = |x: f32| {
            let phase = x / width * 12.0 + t * speed + offset;
            height * (0.5 - 0.35 * phase.sin() * (0.6 + 0.4 * (phase * 0.37).cos()))
        };
        let mut previous = (0.0, y(0.0));
        for x in 1..canvas.width() {
            let point = (x as f32, y(x as f32));
            canvas.line_aa(previous.0, previous.1, point.0, point.1, color);
            previous = point;
        }
    }
    canvas.render(buffer, inner, mode, &RenderOptions::default());
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use vnce::vnce_runtime::{RunConfig, ScriptedDriver, run_with};
    use vnce::vnce_term::input::{MouseButton, MouseEvent, MouseEventKind};
    use vnce::vnce_term::{
        Capabilities, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, Terminal, TestBackend,
    };

    use super::{Demo, Focus};

    /// Presses `code` at `at` milliseconds.
    fn press(driver: &mut ScriptedDriver, at: u64, code: KeyCode) {
        driver.push_event(
            Duration::from_millis(at),
            Event::Key(KeyEvent {
                code,
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Press,
            }),
        );
    }

    /// Clicks the cell `(column, row)` at `at` milliseconds.
    fn click(driver: &mut ScriptedDriver, at: u64, column: u16, row: u16) {
        for (offset, kind) in [
            (0, MouseEventKind::Moved),
            (30, MouseEventKind::Down(MouseButton::Left)),
            (60, MouseEventKind::Up(MouseButton::Left)),
        ] {
            driver.push_event(
                Duration::from_millis(at + offset),
                Event::Mouse(MouseEvent {
                    kind,
                    column,
                    row,
                    modifiers: KeyModifiers::NONE,
                }),
            );
        }
    }

    /// Opens the controls page from the menu, types into the name, changes the volume and
    /// the checkbox, saves, and opens the scores with a click on their tab.
    #[test]
    fn smoke_test_controls_and_clicks() {
        let mut demo = Demo::new();
        let terminal = Terminal::new(TestBackend::new(90, 30), Capabilities::default())
            .expect("the test backend has a size");
        let mut driver = ScriptedDriver::new();
        let mut at = 100;
        let mut key = |driver: &mut ScriptedDriver, code| {
            press(driver, at, code);
            at += 50;
        };
        key(&mut driver, KeyCode::Down);
        key(&mut driver, KeyCode::Enter);
        key(&mut driver, KeyCode::Tab);
        for code in [
            KeyCode::Char('!'),
            KeyCode::Char('q'),
            KeyCode::Backspace,
            KeyCode::Tab,
            KeyCode::Right,
            KeyCode::Right,
            KeyCode::Tab,
            KeyCode::Char(' '),
            KeyCode::Tab,
            KeyCode::Enter,
        ] {
            key(&mut driver, code);
        }
        // The tabs start after the 24-cell menu and a space, in the second row:
        // " Dashboard │ Controls │ High scores ".
        click(&mut driver, 1000, 25 + 26, 1);
        // The third score row: the table starts in row 2 (border, header, rows).
        click(&mut driver, 1200, 30, 6);
        press(&mut driver, 1500, KeyCode::Char('q'));
        run_with(&mut demo, RunConfig::default(), terminal, driver).expect("the demo runs");

        assert_eq!(demo.name.text(), "Player!", "'q' was typed and erased");
        assert!((demo.volume - 80.0).abs() < 1e-9);
        assert!(!demo.sound);
        assert!(demo.saved.as_deref().is_some_and(|s| s.contains("Player!")));
        assert_eq!(demo.page, 2, "the scores tab was clicked");
        assert_eq!(demo.focus, Focus::Scores);
        assert_eq!(demo.scores.selected(), Some(2));
    }
}
