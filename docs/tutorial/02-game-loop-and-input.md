# 2. Game loop and input

## Fixed updates, smooth frames

The engine separates **updates** from **frames**:

- `Game::update` runs at a fixed rate (`RunConfig::update_rate`, 60 by default). Every
  update advances the game by exactly `ctx.dt()` (`ctx.dt_secs()` as `f32`), so the game
  runs at the same speed on a fast and on a slow machine.
- `Game::render` runs at the frame limit (`RunConfig::fps`). Frames can fall between two
  updates: `ctx.alpha()` (`0.0..1.0`) tells how far, so you can draw positions between the
  previous and the current update for smooth movement at any frame rate.

The pattern is: remember the previous position in `update`, mix it in `render`.

```rust,no_run
use vnce::vnce_core::Style;
use vnce::vnce_runtime::{self, Context, Game, RunConfig};
use vnce::vnce_term::{Frame, KeyCode};

/// Cells per second. Cells are about twice as tall as wide, so vertical speed is halved.
const SPEED: (f32, f32) = (24.0, 12.0);

struct Walker {
    position: (f32, f32),
    previous: (f32, f32),
}

impl Game for Walker {
    fn update(&mut self, ctx: &mut Context) {
        self.previous = self.position;
        let input = ctx.input();
        let mut direction = (0.0, 0.0);
        if input.is_down(KeyCode::Left) {
            direction.0 -= 1.0;
        }
        if input.is_down(KeyCode::Right) {
            direction.0 += 1.0;
        }
        if input.is_down(KeyCode::Up) {
            direction.1 -= 1.0;
        }
        if input.is_down(KeyCode::Down) {
            direction.1 += 1.0;
        }
        let dt = ctx.dt_secs();
        self.position.0 += direction.0 * SPEED.0 * dt;
        self.position.1 += direction.1 * SPEED.1 * dt;
        if input.was_pressed(KeyCode::Esc) {
            ctx.quit();
        }
    }

    fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>) {
        let a = ctx.alpha();
        let x = self.previous.0 + (self.position.0 - self.previous.0) * a;
        let y = self.previous.1 + (self.position.1 - self.previous.1) * a;
        let (x, y) = (x.round().max(0.0) as u16, y.round().max(0.0) as u16);
        frame.buffer_mut().set_string(x, y, "@", Style::new().bold());
    }
}

fn main() -> Result<(), vnce_runtime::RunError> {
    let start = (10.0, 5.0);
    let mut game = Walker { position: start, previous: start };
    vnce_runtime::run(&mut game, RunConfig::default())
}
```

## Reading the keyboard

`ctx.input()` is the `InputState` of the current update:

| Method | True when |
| --- | --- |
| `is_down(key)` | the key is held (movement, charging) |
| `was_pressed(key)` | the key went down since the previous update (jump, menu) |
| `was_released(key)` | the key went up since the previous update |
| `is_mouse_down(button)`, `mouse_position()`, `scroll()` | mouse state, with mouse capture on |

Letters are reported as typed: `Char('a')`, or `Char('A')` with Shift held.

Not every terminal reports key releases. Where they are missing, the engine counts a key
as released when it stops repeating (`RunConfig::key_hold_timeout`), so `is_down` works
everywhere; `input.key_release_events()` tells which case you are in. Holding two keys at
once is only reliable where releases are reported (Windows, and terminals with the kitty
keyboard protocol).

The methods that change the loop (`quit`, `set_paused`, `set_fps_limit`,
`set_update_rate`, `set_graphics_mode`) can be called while `input` is still in use, as
above.

## Actions instead of keys

Hard-coded keys are hard to change later. `ActionMap` (in `vnce_game`) maps your own
actions to keys and mouse buttons; the game asks about actions, and players can rebind
them (chapter 6 saves them).

```rust
use vnce::vnce_game::ActionMap;
use vnce::vnce_term::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Left,
    Right,
    Jump,
}

let controls = ActionMap::new()
    .bind(Action::Left, KeyCode::Left)
    .bind(Action::Left, 'a')
    .bind(Action::Right, KeyCode::Right)
    .bind(Action::Right, 'd')
    .bind(Action::Jump, ' ');
assert_eq!(controls.bindings(Action::Left).count(), 2);

// In `update`:
// let dx = controls.axis(Action::Left, Action::Right, ctx.input()); // -1.0, 0.0 or 1.0
// if controls.was_pressed(Action::Jump, ctx.input()) { /* jump */ }
```

Letters in an `ActionMap` match both cases, so Caps Lock does not break the controls.

In a native window (see [Shipping a game](../SHIPPING-GAMES.md)) gamepads work too: bind
`GamepadButton::South` or a stick direction (`Binding::stick(GamepadAxis::LeftX, false)`)
to the same actions, and `axis` becomes smooth with a stick. In a terminal those
bindings never fire, so they do no harm.

## Events

For things that should happen exactly once per key press, independent of updates (typing
a name, opening a menu while paused), implement `Game::event`. It receives every event as
it arrives:

```rust
use vnce::vnce_runtime::{Context, Game};
use vnce::vnce_term::{Event, Frame, KeyCode};

struct NameInput {
    name: String,
}

impl Game for NameInput {
    fn event(&mut self, _ctx: &mut Context, event: &Event) {
        if let Event::Key(key) = event {
            if !key.is_press() {
                return; // Windows also reports releases
            }
            match key.code {
                KeyCode::Char(c) => self.name.push(c),
                KeyCode::Backspace => {
                    self.name.pop();
                }
                _ => {}
            }
        }
    }

    fn update(&mut self, _ctx: &mut Context) {}

    fn render(&mut self, _ctx: &Context, _frame: &mut Frame<'_>) {}
}
```

Next: [Pixel graphics](03-pixel-graphics.md).
