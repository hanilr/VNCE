//! Collect the stars: a small game that exercises the runtime.
//!
//! - Movement uses held keys (`InputState::is_down`) at a fixed update rate, so the speed
//!   is the same at every frame rate.
//! - Rendering interpolates the player position with `Context::alpha`.
//! - `+` / `-` change the frame limit while the game runs (15 FPS up to unlimited).
//!
//! Run with:
//!
//! ```sh
//! cargo run --example stars
//! ```
//!
//! Arrow keys / WASD: move · `+` / `-`: FPS limit · `P`: pause · `Esc` / `Q`: quit.

use std::collections::VecDeque;

use vnce::vnce_core::{Buffer, Color, Position, Rect, Style};
use vnce::vnce_game::Rng;
use vnce::vnce_runtime::{self, Context, FpsLimit, Game, RunConfig, RunError};
use vnce::vnce_term::{Event, Frame, KeyCode, KeyEventKind};

/// Player speed in cells per second. Terminal cells are about twice as tall as they are
/// wide, so the vertical speed is halved to make diagonal movement look even.
const SPEED_X: f32 = 24.0;
const SPEED_Y: f32 = 12.0;

/// Number of stars on screen.
const STAR_COUNT: usize = 6;

/// How many updates a trail cell stays visible.
const TRAIL_LIFETIME: u16 = 45;

/// Frame limits selectable with `+` / `-`.
const FPS_STEPS: [FpsLimit; 7] = [
    FpsLimit::Fixed(15),
    FpsLimit::Fixed(30),
    FpsLimit::Fixed(60),
    FpsLimit::Fixed(120),
    FpsLimit::Fixed(144),
    FpsLimit::Fixed(240),
    FpsLimit::Unlimited,
];

fn main() -> Result<(), RunError> {
    vnce_runtime::run(&mut StarsGame::new(), RunConfig::default())
}

/// A position in cells, with sub-cell precision.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Vec2 {
    x: f32,
    y: f32,
}

impl Vec2 {
    fn lerp(self, to: Self, t: f32) -> Self {
        Self {
            x: self.x + (to.x - self.x) * t,
            y: self.y + (to.y - self.y) * t,
        }
    }

    /// The cell that contains this position.
    fn cell(self) -> (u16, u16) {
        (to_cell(self.x), to_cell(self.y))
    }
}

/// Rounds a non-negative coordinate to a cell index.
fn to_cell(value: f32) -> u16 {
    // The value is clamped to the u16 range first, so the conversion cannot truncate.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is clamped to the u16 range"
    )]
    let cell = value.round().clamp(0.0, f32::from(u16::MAX)) as u16;
    cell
}

/// A random value in `low..high` (`low` if the range is empty), from the engine's tiny
/// xorshift random number generator (no dependency needed for a demo).
fn random_in(rng: &mut Rng, low: u16, high: u16) -> u16 {
    u16::try_from(rng.range(i32::from(low)..i32::from(high))).unwrap_or(low)
}

/// One cell of the fading trail behind the player.
struct TrailCell {
    x: u16,
    y: u16,
    age: u16,
}

struct StarsGame {
    rng: Rng,
    /// Position before the last update, for interpolation.
    previous: Vec2,
    player: Vec2,
    stars: Vec<(u16, u16)>,
    trail: VecDeque<TrailCell>,
    score: u32,
    fps_step: usize,
}

impl StarsGame {
    fn new() -> Self {
        Self {
            rng: Rng::from_time(),
            previous: Vec2 { x: 0.0, y: 0.0 },
            player: Vec2 { x: 0.0, y: 0.0 },
            stars: Vec::new(),
            trail: VecDeque::new(),
            score: 0,
            fps_step: 2,
        }
    }

    /// The playing field: everything inside the border, below the status line.
    fn field(ctx: &Context) -> Rect {
        let size = ctx.size();
        Rect::new(0, 1, size.width, size.height.saturating_sub(2)).inner(1, 1)
    }

    fn random_star(&mut self, field: Rect) -> (u16, u16) {
        (
            random_in(&mut self.rng, field.left(), field.right()),
            random_in(&mut self.rng, field.top(), field.bottom()),
        )
    }

    /// Keeps the player and the stars inside the field (after a resize).
    fn keep_inside(&mut self, field: Rect) {
        let clamp = |value: f32, low: u16, high: u16| {
            value.clamp(f32::from(low), f32::from(high.saturating_sub(1).max(low)))
        };
        self.player.x = clamp(self.player.x, field.left(), field.right());
        self.player.y = clamp(self.player.y, field.top(), field.bottom());
        for index in 0..self.stars.len() {
            let (x, y) = self.stars[index];
            if !field.contains(Position::new(x, y)) {
                self.stars[index] = self.random_star(field);
            }
        }
        self.trail
            .retain(|t| field.contains(Position::new(t.x, t.y)));
    }

    fn change_fps(&mut self, ctx: &mut Context, up: bool) {
        self.fps_step = if up {
            (self.fps_step + 1).min(FPS_STEPS.len() - 1)
        } else {
            self.fps_step.saturating_sub(1)
        };
        // Every step is a valid limit, so this cannot fail.
        let _ = ctx.set_fps_limit(FPS_STEPS[self.fps_step]);
    }
}

impl Game for StarsGame {
    fn init(&mut self, ctx: &mut Context) {
        let field = Self::field(ctx);
        self.player = Vec2 {
            x: f32::from(field.x + field.width / 2),
            y: f32::from(field.y + field.height / 2),
        };
        self.previous = self.player;
        for _ in 0..STAR_COUNT {
            let star = self.random_star(field);
            self.stars.push(star);
        }
    }

    fn event(&mut self, ctx: &mut Context, event: &Event) {
        // One-off actions react to the press itself, not to repeats or releases.
        let Event::Key(key) = event else {
            return;
        };
        if key.kind != KeyEventKind::Press {
            return;
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q' | 'Q') => ctx.quit(),
            KeyCode::Char('p' | 'P') => ctx.set_paused(!ctx.is_paused()),
            // `=` is the unshifted `+` key (reported that way with keyboard enhancement).
            KeyCode::Char('+' | '=') => self.change_fps(ctx, true),
            KeyCode::Char('-' | '_') => self.change_fps(ctx, false),
            _ => {}
        }
    }

    fn update(&mut self, ctx: &mut Context) {
        let field = Self::field(ctx);
        if field.is_empty() {
            return;
        }
        self.keep_inside(field);
        self.previous = self.player;

        // Direction from held keys.
        let input = ctx.input();
        let axis =
            |negative: bool, positive: bool| f32::from(i8::from(positive) - i8::from(negative));
        let mut dx = axis(
            input.any_down(&[KeyCode::Left, KeyCode::Char('a')]),
            input.any_down(&[KeyCode::Right, KeyCode::Char('d')]),
        );
        let mut dy = axis(
            input.any_down(&[KeyCode::Up, KeyCode::Char('w')]),
            input.any_down(&[KeyCode::Down, KeyCode::Char('s')]),
        );
        if dx != 0.0 && dy != 0.0 {
            // Diagonal movement is not faster than straight movement.
            dx *= std::f32::consts::FRAC_1_SQRT_2;
            dy *= std::f32::consts::FRAC_1_SQRT_2;
        }

        let dt = ctx.dt_secs();
        self.player.x += dx * SPEED_X * dt;
        self.player.y += dy * SPEED_Y * dt;
        self.keep_inside(field);

        // Trail: one cell per visited position, fading with age.
        for cell in &mut self.trail {
            cell.age += 1;
        }
        while self.trail.front().is_some_and(|t| t.age > TRAIL_LIFETIME) {
            self.trail.pop_front();
        }
        let (x, y) = self.player.cell();
        if (dx != 0.0 || dy != 0.0) && self.trail.back().is_none_or(|t| (t.x, t.y) != (x, y)) {
            self.trail.push_back(TrailCell { x, y, age: 0 });
        }

        // Collect stars.
        for index in 0..self.stars.len() {
            if self.stars[index] == (x, y) {
                self.score += 1;
                self.stars[index] = self.random_star(field);
            }
        }
    }

    fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>) {
        let area = frame.area();
        let field = Self::field(ctx);
        let buffer = frame.buffer_mut();

        draw_status(buffer, ctx, self.score, area.width);
        if field.width < 10 || field.height < 3 {
            buffer.set_string(
                1,
                1,
                "Terminal too small",
                Style::new().fg(Color::BrightRed),
            );
            return;
        }
        draw_border(
            buffer,
            Rect::new(0, 1, area.width, area.height.saturating_sub(2)),
        );

        for cell in &self.trail {
            let fade = 1.0 - f32::from(cell.age) / f32::from(TRAIL_LIFETIME);
            if let Some(target) = buffer.cell_mut(cell.x, cell.y) {
                target.bg = Color::rgb(
                    channel(40.0 + 160.0 * fade),
                    channel(20.0 + 60.0 * fade),
                    channel(60.0 * (1.0 - fade)),
                );
            }
        }

        let star_style = Style::new().fg(Color::BrightYellow).bold();
        for &(x, y) in &self.stars {
            buffer.set_string(x, y, "*", star_style);
        }

        // Interpolated position: smooth movement even when FPS > UPS.
        let (x, y) = self.previous.lerp(self.player, ctx.alpha()).cell();
        buffer.set_string(x, y, "@", Style::new().fg(Color::BrightCyan).bold());

        let help = "Arrows/WASD: move · +/-: FPS limit · P: pause · Esc/Q: quit";
        buffer.set_string(1, area.height.saturating_sub(1), help, Style::new().dim());
    }
}

fn draw_status(buffer: &mut Buffer, ctx: &Context, score: u32, width: u16) {
    let style = Style::new()
        .fg(Color::rgb(20, 20, 30))
        .bg(Color::rgb(255, 200, 90));
    buffer.set_style(Rect::new(0, 0, width, 1), style);
    let keys = if ctx.input().key_release_events() {
        "key releases"
    } else {
        "key timeout"
    };
    let paused = if ctx.is_paused() { " · PAUSED" } else { "" };
    let status = format!(
        " Score {score} · FPS {:.0} / {} · UPS {:.0} · {keys}{paused}",
        ctx.fps(),
        ctx.fps_limit(),
        ctx.ups(),
    );
    buffer.set_string(0, 0, &status, style.bold());
}

fn draw_border(buffer: &mut Buffer, area: Rect) {
    if area.width < 2 || area.height < 2 {
        return;
    }
    let style = Style::new().fg(Color::rgb(90, 90, 120));
    let (left, top) = (area.left(), area.top());
    let (right, bottom) = (area.right() - 1, area.bottom() - 1);
    for x in left + 1..right {
        buffer.set_string(x, top, "─", style);
        buffer.set_string(x, bottom, "─", style);
    }
    for y in top + 1..bottom {
        buffer.set_string(left, y, "│", style);
        buffer.set_string(right, y, "│", style);
    }
    buffer.set_string(left, top, "┌", style);
    buffer.set_string(right, top, "┐", style);
    buffer.set_string(left, bottom, "└", style);
    buffer.set_string(right, bottom, "┘", style);
}

/// Converts a color channel in `0.0..=255.0` to a byte.
fn channel(value: f32) -> u8 {
    // The value is clamped to 0..=255 first, so the conversion cannot truncate.
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is clamped to the u8 range"
    )]
    let byte = value.round().clamp(0.0, 255.0) as u8;
    byte
}
