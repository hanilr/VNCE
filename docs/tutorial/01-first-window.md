# 1. First window

A program built with the engine is a type that implements the `Game` trait. The engine
owns the loop: it calls your game to **update** the world and to **render** it, and it
takes care of the terminal (raw mode, the alternate screen, restoring everything when the
program ends, even after a panic).

## The smallest game

```rust,no_run
use vnce::vnce_core::{Color, Style};
use vnce::vnce_runtime::{self, Context, Game, RunConfig};
use vnce::vnce_term::{Frame, KeyCode};

struct Hello {
    updates: u64,
}

impl Game for Hello {
    fn update(&mut self, ctx: &mut Context) {
        self.updates += 1;
        if ctx.input().was_pressed(KeyCode::Char('q')) {
            ctx.quit();
        }
    }

    fn render(&mut self, _ctx: &Context, frame: &mut Frame<'_>) {
        let style = Style::new().fg(Color::rgb(255, 210, 110)).bold();
        let buffer = frame.buffer_mut();
        buffer.set_string(2, 1, "Hello from the terminal!", style);
        buffer.set_string(2, 3, &format!("updates: {}", self.updates), Style::new());
        buffer.set_string(2, 5, "press q to quit", Style::new().dim());
    }
}

fn main() -> Result<(), vnce_runtime::RunError> {
    vnce_runtime::run(&mut Hello { updates: 0 }, RunConfig::default())
}
```

Run it with `cargo run`. `Ctrl+C` also quits (see `RunConfig::quit_on_ctrl_c`).

## What happens here

- `update` runs **60 times per second** by default, on every machine. Game logic goes here.
- `render` draws the current state. The frame starts empty every time, so you always draw
  everything; the engine compares it with the previous frame and only sends the cells
  that changed to the terminal.
- `frame.buffer_mut()` is a grid of cells. `set_string(x, y, text, style)` writes text;
  wide characters (emoji, CJK) take two cells and are handled for you.
- `ctx` (the `Context`) gives the time, the input and the settings of the loop.

## Styles and colors

`Style` sets the colors and modifiers of text. Colors can be named, 256-color indices or
true color; the engine lowers them automatically on terminals with fewer colors.

```rust
use vnce::vnce_core::{Buffer, Color, Rect, Size, Style};

let mut buffer = Buffer::empty(Size::new(20, 2));
let title = Style::new().fg(Color::rgb(20, 16, 24)).bg(Color::rgb(140, 200, 255)).bold();
buffer.set_style(Rect::new(0, 0, 20, 1), title); // a colored bar
buffer.set_string(1, 0, "My game", title);
assert_eq!(buffer.cell(1, 0).unwrap().symbol(), "M");
```

`Buffer` is also what you test against: rendering into a buffer needs no terminal.

## The configuration

`RunConfig` changes how the loop runs; every field has a sensible default:

```rust
use vnce::vnce_runtime::{FpsLimit, RunConfig};

let config = RunConfig {
    update_rate: 60,                 // updates per second
    fps: FpsLimit::Fixed(120),       // frames per second (or FpsLimit::Unlimited)
    pause_on_focus_lost: true,       // stop updating when the window loses focus
    ..RunConfig::default()
};
assert!(config.validate().is_ok());
```

Next: [Game loop and input](02-game-loop-and-input.md).
