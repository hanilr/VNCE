//! A dashboard built with the widgets of `vnce_ui`: a menu, gauges, a live pixel chart,
//! wrapped scrolling text, a popup and an FPS overlay.
//!
//! Run with:
//!
//! ```sh
//! cargo run --example ui
//! ```
//!
//! `↑` / `↓` (or `W` / `S`): move in the menu · `Enter`: choose · `P`: popup ·
//! `F`: FPS overlay · `Esc`: close the popup or quit · `Q`: quit.

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
use vnce::vnce_term::{Event, Frame, KeyCode, KeyEventKind};
use vnce::vnce_ui::{
    Alignment, Block, BorderType, Clear, Constraint, FpsOverlay, Gauge, Layout, Line, List,
    ListState, Padding, Paragraph, Span, StatefulWidget, Widget,
};

const MENU: [&str; 4] = ["Dashboard", "Show a popup", "Toggle FPS overlay", "Quit"];

const STORY: &str = "Variation draws everything into one buffer of cells: widgets, pixel \
graphics, GPU frames and images can share a frame. Widgets are immediate mode: they are \
built and drawn again on every frame, and only the cells that changed reach the terminal. \
Text wraps at word boundaries, keeps wide characters such as 日本語 whole, and lines that \
do not fit end with an ellipsis.";

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut demo = Demo {
        menu: ListState::with_selected(Some(0)),
        popup: false,
        overlay: true,
        mode: PixelMode::HalfBlock,
        last_frame: None,
        frame_times: VecDeque::new(),
    };
    vnce::run(&mut demo, RunConfig::default())?;
    Ok(())
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
}

impl Game for Demo {
    fn init(&mut self, ctx: &mut Context) {
        self.mode = PixelMode::detect(ctx.capabilities().color_depth);
    }

    fn event(&mut self, ctx: &mut Context, event: &Event) {
        let Event::Key(key) = event else {
            return;
        };
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Esc if self.popup => self.popup = false,
            KeyCode::Esc | KeyCode::Char('q' | 'Q') => ctx.quit(),
            KeyCode::Up | KeyCode::Char('w' | 'W') => self.menu.select_previous(MENU.len(), true),
            KeyCode::Down | KeyCode::Char('s' | 'S') => self.menu.select_next(MENU.len(), true),
            KeyCode::Char('p' | 'P') => self.popup = !self.popup,
            KeyCode::Char('f' | 'F') => self.overlay = !self.overlay,
            KeyCode::Enter => match self.menu.selected() {
                Some(1) => self.popup = true,
                Some(2) => self.overlay = !self.overlay,
                Some(3) => ctx.quit(),
                _ => self.popup = false,
            },
            _ => {}
        }
    }

    fn update(&mut self, _ctx: &mut Context) {}

    fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>) {
        let now = Instant::now();
        if let Some(last) = self.last_frame.replace(now) {
            self.frame_times
                .push_back((now - last).as_secs_f32() * 1000.0);
            if self.frame_times.len() > 24 {
                self.frame_times.pop_front();
            }
        }
        let t = ctx.elapsed().as_secs_f32();
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
        Paragraph::new(" ↑↓: menu · Enter: choose · P: popup · F: FPS overlay · Esc / Q: quit")
            .style(Style::new().dim())
            .render(help, buffer);

        let [menu_area, main] = Layout::horizontal([Constraint::Length(24), Constraint::Fill(1)])
            .spacing(1)
            .areas(body);
        let menu = List::new(MENU)
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
            );
        menu.render(menu_area, buffer, &mut self.menu);

        let [gauges, chart, text] = Layout::vertical([
            Constraint::Length(5),
            Constraint::Fill(1),
            Constraint::Length(6),
        ])
        .areas(main);
        render_gauges(gauges, buffer, t);
        render_chart(chart, buffer, t, self.mode);
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

        if self.overlay {
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
