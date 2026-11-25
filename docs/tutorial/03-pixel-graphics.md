# 3. Pixel graphics

Text cells are coarse. For pixel art, draw on a `Canvas` (a grid of `Rgba` pixels) and
hand it to the frame; the engine shows it with the best **display method** of the
terminal.

## The canvas

```rust
use vnce::vnce_raster::{BlendMode, Canvas, Font, Rgba};

let mut canvas = Canvas::filled(64, 36, Rgba::rgb(20, 18, 30)); // sky
canvas.fill_rect(0, 28, 64, 8, Rgba::rgb(60, 140, 70)); // ground
canvas.fill_circle(50, 8, 5, Rgba::rgb(255, 220, 120)); // sun
canvas.line(0, 27, 63, 20, Rgba::WHITE); // a one-pixel line
canvas.line_aa(0.0, 30.0, 63.0, 24.0, Rgba::WHITE); // an anti-aliased one
canvas.fill_triangle((10.0, 28.0), (20.0, 12.0), (30.0, 28.0), Rgba::rgb(90, 90, 110));

// Semi-transparent colors blend; `BlendMode::Add` makes light.
canvas.set_blend_mode(BlendMode::Add);
canvas.fill_circle(50, 8, 8, Rgba::new(255, 200, 80, 60)); // glow
canvas.set_blend_mode(BlendMode::Alpha);

// Text in pixels, with the built-in 5×7 font.
canvas.text(Font::builtin(), 2, 2, "SCORE 120", Rgba::WHITE);
canvas.text_scaled(Font::builtin(), 2, 12, "GO!", Rgba::rgb(255, 120, 100), 2);

assert_eq!(canvas.get(0, 35), Some(Rgba::rgb(60, 140, 70)));
```

Coordinates are signed and everything is clipped: shapes may be partly or completely off
the canvas. `set_clip` limits drawing to a rectangle. PNG files load with
`Canvas::load_png("player.png")` (or `Canvas::from_png(bytes)` for embedded files).

## Showing a canvas

`frame.draw_image(area, &canvas)` scales the canvas to an area of cells:

```rust,no_run
use vnce::vnce_core::Rect;
use vnce::vnce_raster::{Canvas, Rgba};
use vnce::vnce_runtime::{self, Context, Game, RunConfig};
use vnce::vnce_term::Frame;

struct Picture {
    time: f32,
}

impl Game for Picture {
    fn update(&mut self, ctx: &mut Context) {
        self.time += ctx.dt_secs();
    }

    fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>) {
        let size = ctx.size();
        let area = Rect::new(0, 0, size.width, size.height);
        // Draw at the size the terminal shows without scaling: the sharpest result.
        let (width, height) = frame.image_pixel_size(area);
        let mut canvas = Canvas::filled(width.max(1), height.max(1), Rgba::BLACK);
        let x = (width as f32 / 2.0 + self.time.sin() * width as f32 / 3.0) as i32;
        canvas.fill_circle(x, height as i32 / 2, height / 4, Rgba::rgb(255, 210, 110));
        frame.draw_image(area, &canvas);
    }
}

fn main() -> Result<(), vnce_runtime::RunError> {
    vnce_runtime::run(&mut Picture { time: 0.0 }, RunConfig::default())
}
```

Pixel art usually looks best at a small fixed size (for example 160 × 90) scaled up;
`frame.image_pixel_size(area)` tells you how many screen pixels there are, so you can
pick a whole-number scale.

## Display methods

`GraphicsMode` is how images reach the screen:

- **Graphics protocols** (`Protocol(Kitty)`, `Protocol(Sixel)`, `Protocol(Iterm2)`) send
  real pixels. Best quality; Kitty is also the fastest.
- **Cells** (`Cells(PixelMode::HalfBlock)`, `Quadrant`, `Sextant`, `Braille`, `Ascii`, …)
  build the picture from characters and colors. Works in every terminal and over SSH;
  half blocks keep every pixel's color.

The engine picks the best mode at start. Players can force one with the environment
variable `VNCE_GRAPHICS` (`kitty`, `sixel`, `iterm2`, `halfblock`, `quadrant`, …). Your
game can list and switch modes while it runs:

```rust
use vnce::vnce_raster::PixelMode;
use vnce::vnce_term::GraphicsMode;

let mode: GraphicsMode = "halfblock".parse().unwrap();
assert_eq!(mode, GraphicsMode::Cells(PixelMode::HalfBlock));
assert!("kitty".parse::<GraphicsMode>().unwrap().is_protocol());

// In a game:
// let modes = GraphicsMode::available(ctx.terminal_info(), &ctx.capabilities());
// ctx.set_graphics_mode(modes[0]); // applied from the next frame
```

Pixels are not always square: a quadrant pixel is half a cell wide and half a cell high.
`frame.image_pixel_aspect()` gives the width-to-height ratio of one pixel, so a camera
can show the world without stretching it.

Next: [Sprites and animation](04-sprites-and-animation.md).
