//! The two galleries opened from the station: the prism screen (every pixel mode, real
//! pixels, scaling) and the archive terminal (widgets and text).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use vnce::vnce_core::{Color, Modifiers, Rect, Style};
use vnce::vnce_game::{Scene, Transition};
use vnce::vnce_raster::{Canvas, Font, PixelMode, Quality, RenderOptions, Rgba, ScaleQuality};
use vnce::vnce_runtime::Context;
use vnce::vnce_term::input::MouseButton;
use vnce::vnce_term::{Frame, GraphicsMode, KeyCode};
use vnce::vnce_ui::{
    Block, BorderType, Constraint, Corner, FpsOverlay, Gauge, Layout, Line, List, ListState,
    Padding, Paragraph, Span, StatefulWidget, Text, Widget,
};

use crate::play::hue_color;
use crate::{ACCENT, Action, COOL, PANEL, Shared, help_line, status_line};

/// Whether the gallery should close: Esc, or the use key / a click.
fn closing(shared: &Shared, ctx: &Context) -> bool {
    let input = ctx.input();
    shared.controls.was_pressed(Action::Back, input)
        || shared.controls.was_pressed(Action::Quit, input)
}

fn titled(title: String, accent: Color) -> Block<'static> {
    Block::bordered()
        .border_type(BorderType::Rounded)
        .border_style(Style::new().fg(accent))
        .title(Line::from(title))
}

// ---------------------------------------------------------------------------------------
// Prism
// ---------------------------------------------------------------------------------------

pub struct Prism {
    /// The test picture shown in every mode.
    picture: Canvas,
    /// Small pixel art for the scaling comparison.
    sprite: Canvas,
    /// The picture scaled to each panel size (it only changes on resize).
    scaled: HashMap<(u32, u32), Canvas>,
    work: Canvas,
    dither: bool,
    quality: Quality,
    /// Running averages of fast and exact scaling, in milliseconds.
    fast_ms: f32,
    exact_ms: f32,
    time: f32,
}

impl Prism {
    pub fn new() -> Self {
        Self {
            picture: test_picture(),
            sprite: Canvas::from_art(
                &[
                    "....kkkk....",
                    "..kkyyyykk..",
                    ".kyyyyyyyyk.",
                    ".kyykyykyyk.",
                    "kyyyyyyyyyyk",
                    "kyyyyyyyyyyk",
                    "kyykyyyykyyk",
                    ".kyykkkkyyk.",
                    ".kyyyyyyyyk.",
                    "..kkyyyykk..",
                    "....kkkk....",
                ],
                &[('k', Rgba::rgb(30, 20, 40)), ('y', Rgba::rgb(255, 205, 70))],
            ),
            scaled: HashMap::new(),
            work: Canvas::new(0, 0),
            dither: false,
            quality: Quality::Fast,
            fast_ms: 0.0,
            exact_ms: 0.0,
            time: 0.0,
        }
    }

    /// The picture at `width × height` with the moving ball, in `self.work`.
    fn animated(&mut self, width: u32, height: u32) {
        let scaled = self
            .scaled
            .entry((width, height))
            .or_insert_with(|| self.picture.scaled(width, height));
        self.work.clone_from(scaled);
        let (w, h) = (width as f32, height as f32);
        let x = w * (0.5 + 0.35 * (self.time * 0.9).cos());
        let y = h * (0.45 + 0.25 * (self.time * 1.3).sin());
        let radius = (h * 0.12).max(2.0) as u32;
        self.work
            .fill_circle(x as i32, y as i32, radius, Rgba::rgb(255, 255, 255));
        self.work
            .fill_circle(x as i32, y as i32, radius / 2, Rgba::rgb(255, 80, 120));
    }
}

/// A landscape with every kind of detail the pixel modes handle differently: gradients,
/// a sun, slopes, thin lines, small text and pure colors.
fn test_picture() -> Canvas {
    let (width, height) = (96, 56);
    let mut canvas = Canvas::new(width, height);
    for y in 0..height {
        let t = y as f32 / height as f32;
        let sky = Rgba::rgb(40, 60, 140).lerp(Rgba::rgb(250, 150, 120), t * 1.4);
        canvas.fill_rect(0, y as i32, width, 1, sky);
    }
    canvas.fill_circle(70, 20, 9, Rgba::rgb(255, 230, 140));
    canvas.fill_triangle_gradient(
        ((-10.0, 44.0), Rgba::rgb(40, 50, 80)),
        ((30.0, 14.0), Rgba::rgb(200, 210, 240)),
        ((66.0, 44.0), Rgba::rgb(40, 50, 80)),
    );
    canvas.fill_triangle(
        (40.0, 44.0),
        (78.0, 22.0),
        (110.0, 44.0),
        Rgba::rgb(60, 70, 100),
    );
    for i in 0..6 {
        let y = 44 + i * 2;
        let color = Rgba::rgb(30, 90 + i as u8 * 12, 150 + i as u8 * 15);
        canvas.fill_rect(0, y, width, 2, color);
    }
    for i in 0..6 {
        canvas.fill_rect(i * 16, 54, 16, 2, hue_color(i as f32 / 6.0, 1.0));
    }
    canvas.line_aa(4.0, 40.0, 92.0, 30.0, Rgba::rgb(255, 255, 255));
    canvas.text(Font::builtin(), 3, 3, "VNCE", Rgba::WHITE);
    canvas
}

impl Scene<Shared> for Prism {
    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        self.time += ctx.dt_secs();
        let input = ctx.input();
        if input.was_pressed(KeyCode::Char('d')) {
            self.dither = !self.dither;
        }
        if input.was_pressed(KeyCode::Char('c')) {
            self.quality = match self.quality {
                Quality::Fast => Quality::Exact,
                Quality::Exact => Quality::Fast,
            };
        }
        if shared.controls.was_pressed(Action::Method, input) {
            shared.cycle_method(ctx);
        }
        if closing(shared, ctx) {
            return Transition::Pop;
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let started = Instant::now();
        let size = ctx.size();
        status_line(
            frame,
            " PRISM HALL · one picture in every pixel mode, then real pixels and scaling",
        );
        help_line(
            frame,
            " D: dithering · C: two-color quality · G: display method · Esc: back to the station",
        );
        let main = Rect::new(0, 1, size.width, size.height.saturating_sub(2));
        let [grid, bottom] =
            Layout::vertical([Constraint::Percentage(64), Constraint::Fill(1)]).areas(main);
        let rows = Layout::vertical([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).split(grid);
        let options = RenderOptions {
            quality: self.quality,
            dither: self.dither,
            ..RenderOptions::default()
        };
        for (index, &mode) in PixelMode::ALL.iter().enumerate() {
            let row = rows[index / 3];
            let columns = Layout::horizontal([Constraint::Ratio(1, 3); 3]).split(row);
            let area = columns[index % 3];
            let (cw, ch) = mode.cell_size();
            let shown = shared.modes.contains(&GraphicsMode::Cells(mode));
            let block = titled(
                format!(" {mode} {cw}×{ch} "),
                if shown { COOL } else { ACCENT },
            )
            .title_bottom(Line::from(if shown {
                " shown correctly "
            } else {
                " needs font support "
            }));
            let inner = block.inner(area);
            block.render(area, frame.buffer_mut());
            if inner.is_empty() {
                continue;
            }
            self.animated(u32::from(inner.width) * cw, u32::from(inner.height) * ch);
            self.work.render(frame.buffer_mut(), inner, mode, &options);
        }

        let [real, scaling] =
            Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).areas(bottom);
        let graphics = frame.graphics_mode();
        let note = if graphics.is_protocol() {
            format!(" real pixels: {graphics} ")
        } else {
            format!(" {graphics}: no graphics protocol here ")
        };
        let block = titled(note, Color::rgb(120, 255, 170));
        let inner = block.inner(real);
        block.render(real, frame.buffer_mut());
        if !inner.is_empty() {
            // The picture at its own size: the terminal scales it to the panel with its
            // scale quality (`VNCE_SCALE_QUALITY`) and sends only the parts that changed.
            self.animated(self.picture.width(), self.picture.height());
            frame.draw_image(inner, &self.work);
        }

        // Fast and exact scaling of the small sprite, side by side, with live timings.
        let [fast, exact] =
            Layout::horizontal([Constraint::Ratio(1, 2), Constraint::Ratio(1, 2)]).areas(scaling);
        for (area, quality) in [(fast, ScaleQuality::Fast), (exact, ScaleQuality::Exact)] {
            let average = if quality == ScaleQuality::Fast {
                self.fast_ms
            } else {
                self.exact_ms
            };
            let block = titled(format!(" {quality} {average:.2} ms "), COOL);
            let inner = block.inner(area);
            block.render(area, frame.buffer_mut());
            if inner.is_empty() {
                continue;
            }
            let (width, height) = (u32::from(inner.width), u32::from(inner.height) * 2);
            let start = Instant::now();
            let scaled = self.sprite.scaled_with(width, height, quality);
            let ms = start.elapsed().as_secs_f32() * 1000.0;
            let slot = if quality == ScaleQuality::Fast {
                &mut self.fast_ms
            } else {
                &mut self.exact_ms
            };
            *slot = if *slot == 0.0 {
                ms
            } else {
                *slot * 0.95 + ms * 0.05
            };
            scaled.render(
                frame.buffer_mut(),
                inner,
                PixelMode::HalfBlock,
                &RenderOptions::default(),
            );
        }
        shared
            .stats
            .borrow_mut()
            .record(started.elapsed(), ctx.fps());
    }
}

// ---------------------------------------------------------------------------------------
// Archive
// ---------------------------------------------------------------------------------------

pub struct Archive {
    topics: ListState,
    scroll: u16,
    cores: u32,
    energy: u32,
    /// Where the topic list was drawn, for mouse clicks.
    list_area: Rect,
}

impl Archive {
    pub fn new(cores: u32, energy: u32) -> Self {
        Self {
            topics: ListState::with_selected(Some(0)),
            scroll: 0,
            cores,
            energy,
            list_area: Rect::default(),
        }
    }
}

const TOPICS: [&str; 5] = [
    "The relay",
    "Türkçe · Unicode",
    "Widgets",
    "The engine",
    "Keys",
];

/// The text of a topic.
#[allow(
    clippy::too_many_lines,
    reason = "the archive's texts, one topic after another"
)]
fn topic(index: usize) -> Text<'static> {
    let bold = Style::new().fg(ACCENT).bold();
    let soft = Style::new().fg(Color::rgb(170, 190, 220));
    let mut text = Text::default();
    let mut push = |line: Line<'static>| text.push_line(line);
    match index {
        0 => {
            push(Line::from(Span::styled("Station log, relay 7", bold)));
            push(Line::from(""));
            push(Line::from(
                "The relay went dark when its five power cores were scattered across the wings. \
                 Each wing still runs one system of the station, and each system guards a core: \
                 the observatory's nebula, the reactor's spinning core, the prism screen, the \
                 sound lab's pads and this archive.",
            ));
            push(Line::from(""));
            push(Line::from(vec![
                Span::raw("Bring all five cores to the "),
                Span::styled("relay", Style::new().fg(Color::rgb(120, 255, 170)).bold()),
                Span::raw(" in the atrium. Drones guard the reactor and never leave it."),
            ]));
        }
        1 => {
            push(Line::from(Span::styled(
                "Türkçe metin ve geniş karakterler",
                bold,
            )));
            push(Line::from(""));
            push(Line::from(
                "Çekirdekleri topla, röleyi yeniden çalıştır! Işığın, gölgenin ve sesin \
                 hepsi terminalde: ğ ü ş ı ö ç Ğ Ü Ş İ Ö Ç.",
            ));
            push(Line::from(""));
            push(Line::from(vec![
                Span::raw("Wide characters stay whole when lines wrap: "),
                Span::styled(
                    "中継局 リレー 📡 ⚡ 🔋",
                    Style::new().fg(Color::rgb(140, 220, 255)),
                ),
                Span::raw(" and so do emoji."),
            ]));
            push(Line::from(""));
            push(Line::from(vec![
                Span::styled("bold", Style::new().bold()),
                Span::raw(" · "),
                Span::styled("italic", Style::new().add_modifier(Modifiers::ITALIC)),
                Span::raw(" · "),
                Span::styled(
                    "underlined",
                    Style::new().add_modifier(Modifiers::UNDERLINE),
                ),
                Span::raw(" · "),
                Span::styled("dim", Style::new().dim()),
                Span::raw(" · "),
                Span::styled("reversed", Style::new().add_modifier(Modifiers::REVERSE)),
                Span::raw(" · "),
                Span::styled(
                    "struck",
                    Style::new().add_modifier(Modifiers::STRIKETHROUGH),
                ),
            ]));
        }
        2 => {
            push(Line::from(Span::styled(
                "What this screen is made of",
                bold,
            )));
            push(Line::from(""));
            for line in [
                "Layout splits the screen with lengths, ratios and fills.",
                "List keeps its selection in a ListState; click a topic with the mouse.",
                "Paragraph wraps, aligns and scrolls (mouse wheel or Page Up / Down).",
                "Gauge fills in eighths of a cell; Block draws five kinds of borders.",
                "FpsOverlay sits in a corner with a sparkline of recent frame rates.",
            ] {
                push(Line::from(vec![Span::styled("• ", soft), Span::raw(line)]));
            }
        }
        3 => {
            push(Line::from(Span::styled(
                "vnce, the Variation CLI Engine",
                bold,
            )));
            push(Line::from(""));
            push(Line::from(
                "Frames are double-buffered cell grids: only changed cells are written, in one \
                 write per frame with synchronized output. Pixels reach the screen as half \
                 blocks, quadrants, sextants, octants, braille or ASCII, or as real pixels \
                 through the Kitty, Sixel and iTerm2 protocols, sending only the parts of an \
                 image that changed.",
            ));
            push(Line::from(""));
            push(Line::from(
                "The game loop updates at a fixed rate and draws at its own; scenes, key \
                 bindings, collision, tile maps, cameras, timers, save files, a mixer with \
                 synthesis and a wgpu renderer complete it.",
            ));
        }
        _ => {
            push(Line::from(Span::styled("Keys", bold)));
            push(Line::from(""));
            for (key, what) in [
                ("←↑→↓ WASD", "move"),
                ("E, click", "use a screen or a terminal"),
                ("P, Esc", "pause"),
                ("G", "next display method"),
                ("F", "performance panel"),
                ("Q", "back to the title"),
            ] {
                push(Line::from(vec![
                    Span::styled(format!("{key:<12}"), Style::new().fg(COOL)),
                    Span::raw(what),
                ]));
            }
        }
    }
    text
}

impl Scene<Shared> for Archive {
    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        let input = ctx.input();
        let before = self.topics.selected();
        if shared.controls.was_pressed(Action::Up, input) {
            self.topics.select_previous(TOPICS.len(), true);
        }
        if shared.controls.was_pressed(Action::Down, input) {
            self.topics.select_next(TOPICS.len(), true);
        }
        // A click on a topic selects it.
        if input.was_mouse_pressed(MouseButton::Left) {
            if let Some(position) = input.mouse_position() {
                let area = self.list_area;
                if area.contains(position) {
                    let row = usize::from(position.y - area.y) + self.topics.offset();
                    if row < TOPICS.len() {
                        self.topics.select(Some(row));
                    }
                }
            }
        }
        let (_, wheel) = input.scroll();
        if input.was_pressed(KeyCode::PageDown) || wheel > 0 {
            self.scroll = self.scroll.saturating_add(3);
        }
        if input.was_pressed(KeyCode::PageUp) || wheel < 0 {
            self.scroll = self.scroll.saturating_sub(3);
        }
        if self.topics.selected() != before {
            self.scroll = 0;
        }
        if closing(shared, ctx) {
            return Transition::Pop;
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, ctx: &Context, frame: &mut Frame<'_>) {
        let started = Instant::now();
        let size = ctx.size();
        status_line(
            frame,
            " ARCHIVE · layouts, text, lists, gauges, borders and the FPS overlay",
        );
        help_line(
            frame,
            " ↑↓ / click: topic · wheel / PgUp PgDn: scroll · Esc: back to the station",
        );
        let main = Rect::new(0, 1, size.width, size.height.saturating_sub(2));
        let [top, gauges, borders] = Layout::vertical([
            Constraint::Fill(1),
            Constraint::Length(3),
            Constraint::Length(3),
        ])
        .areas(main);
        let [list, page] = Layout::horizontal([Constraint::Length(24), Constraint::Fill(1)])
            .spacing(1)
            .areas(top);
        let buffer = frame.buffer_mut();

        let list_block = titled(" Topics ".to_owned(), ACCENT);
        self.list_area = list_block.inner(list);
        List::new(TOPICS)
            .highlight_symbol("▶ ")
            .highlight_style(Style::new().fg(Color::rgb(16, 18, 26)).bg(ACCENT))
            .style(Style::new().bg(PANEL))
            .block(list_block)
            .render(list, buffer, &mut self.topics);

        let selected = self.topics.selected().unwrap_or(0);
        let block =
            titled(format!(" {} ", TOPICS[selected]), COOL).padding(Padding::new(2, 2, 1, 0));
        let text_width = block.inner(page).width;
        let paragraph = Paragraph::new(topic(selected)).wrap(true);
        let lines = paragraph.line_count(text_width) as u16;
        let visible = block.inner(page).height;
        self.scroll = self.scroll.min(lines.saturating_sub(visible));
        paragraph
            .scroll(self.scroll)
            .style(Style::new().bg(PANEL))
            .block(block)
            .render(page, buffer);

        let stats = shared.stats.borrow();
        let frame_budget = f64::from(stats.average_render()) / 16.7;
        let [a, b, c] = Layout::horizontal([Constraint::Ratio(1, 3); 3])
            .spacing(1)
            .areas(gauges);
        for (area, title, ratio, label, color) in [
            (
                a,
                " Cores ",
                f64::from(self.cores) / 5.0,
                format!("{}/5", self.cores),
                Color::rgb(90, 230, 190),
            ),
            (
                b,
                " Energy ",
                f64::from(self.energy) / 3.0,
                format!("{}/3", self.energy),
                Color::rgb(255, 100, 110),
            ),
            (
                c,
                " Frame budget ",
                frame_budget.min(1.0),
                format!("{:.1} of 16.7 ms", stats.average_render()),
                Color::rgb(255, 200, 90),
            ),
        ] {
            Gauge::new()
                .ratio(ratio)
                .label(label)
                .gauge_style(Style::new().fg(color).bg(Color::rgb(30, 32, 44)))
                .block(titled(title.to_owned(), color))
                .render(area, buffer);
        }

        border_kinds(borders, buffer);

        let frame_time = Duration::from_secs_f32(stats.average_render() / 1000.0);
        FpsOverlay::new(ctx.fps())
            .ups(ctx.ups())
            .frame_time(frame_time)
            .history(stats.fps.iter().copied())
            .sparkline_width(16)
            .corner(Corner::TopRight)
            .render(block_inner_top(page), buffer);
        drop(stats);
        shared
            .stats
            .borrow_mut()
            .record(started.elapsed(), ctx.fps());
    }
}

/// One block of each border type, side by side.
fn border_kinds(area: Rect, buffer: &mut vnce::vnce_core::Buffer) {
    let kinds = [
        BorderType::Plain,
        BorderType::Rounded,
        BorderType::Double,
        BorderType::Thick,
        BorderType::Ascii,
    ];
    let cells = Layout::horizontal([Constraint::Ratio(1, 5); 5])
        .spacing(1)
        .split(area);
    for (area, kind) in cells.iter().zip(kinds) {
        Block::bordered()
            .border_type(kind)
            .border_style(Style::new().fg(Color::rgb(170, 180, 210)))
            .title(Line::from(format!(" {kind:?} ")).centered())
            .render(*area, buffer);
    }
}

/// The page area without its border, where the FPS overlay goes.
fn block_inner_top(page: Rect) -> Rect {
    Rect::new(
        page.x + 1,
        page.y + 1,
        page.width.saturating_sub(2),
        page.height.saturating_sub(2),
    )
}
