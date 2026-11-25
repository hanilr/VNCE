# 6. Scenes, UI and saving

## Scenes

A game has screens: a title, the game itself, a pause menu, a game-over screen. Each one
is a `Scene`; a `SceneStack` runs them and is itself a `Game`. Scenes share data (`D`,
for example the controls, the settings and the best score) and return a `Transition`:

| Transition | Effect |
| --- | --- |
| `Transition::None` | stay |
| `Transition::push(scene)` | put a scene on top (pause, settings) |
| `Transition::Pop` | remove the top scene and go back |
| `Transition::switch(scene)` | replace the top scene (title → game) |
| `Transition::Quit` | end the program |

Overlay scenes (`is_overlay` returns `true`) are drawn over the scene below, which stays
visible but frozen: a pause menu over the game.

```rust,no_run
use vnce::vnce_core::Style;
use vnce::vnce_game::{Scene, SceneStack, Transition};
use vnce::vnce_runtime::{self, Context, RunConfig};
use vnce::vnce_term::{Frame, KeyCode};

/// Data shared by every scene.
struct Shared {
    best: u32,
}

struct Title;

impl Scene<Shared> for Title {
    fn update(&mut self, _shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        let input = ctx.input();
        if input.was_pressed(KeyCode::Enter) {
            return Transition::switch(Play { score: 0 });
        }
        if input.was_pressed(KeyCode::Char('q')) {
            return Transition::Quit;
        }
        Transition::None
    }

    fn render(&mut self, shared: &Shared, _ctx: &Context, frame: &mut Frame<'_>) {
        let buffer = frame.buffer_mut();
        buffer.set_string(2, 1, "MY GAME", Style::new().bold());
        buffer.set_string(2, 3, &format!("best: {}", shared.best), Style::new());
        buffer.set_string(2, 5, "Enter: play · q: quit", Style::new().dim());
    }
}

struct Play {
    score: u32,
}

impl Scene<Shared> for Play {
    fn update(&mut self, shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        let input = ctx.input();
        if input.was_pressed(KeyCode::Char(' ')) {
            self.score += 10;
        }
        if input.was_pressed(KeyCode::Char('p')) {
            return Transition::push(Pause);
        }
        if input.was_pressed(KeyCode::Esc) {
            shared.best = shared.best.max(self.score);
            return Transition::switch(Title);
        }
        Transition::None
    }

    fn render(&mut self, _shared: &Shared, _ctx: &Context, frame: &mut Frame<'_>) {
        let text = format!("score {} · space: score · p: pause · Esc: end", self.score);
        frame.buffer_mut().set_string(2, 1, &text, Style::new());
    }
}

struct Pause;

impl Scene<Shared> for Pause {
    fn update(&mut self, _shared: &mut Shared, ctx: &mut Context) -> Transition<Shared> {
        if ctx.input().was_pressed(KeyCode::Char('p')) {
            return Transition::Pop;
        }
        Transition::None
    }

    fn render(&mut self, _shared: &Shared, _ctx: &Context, frame: &mut Frame<'_>) {
        frame.buffer_mut().set_string(2, 3, "PAUSED (p)", Style::new().reverse());
    }

    fn is_overlay(&self) -> bool {
        true
    }
}

fn main() -> Result<(), vnce_runtime::RunError> {
    let mut game = SceneStack::new(Title, Shared { best: 0 });
    vnce_runtime::run(&mut game, RunConfig::default())
}
```

Scenes also have `start` (called when the scene is put on the stack), `resume` (when the
scene above it was popped) and `event` (raw events, as in chapter 2).

## Widgets

`vnce_ui` draws menus, panels and bars into the frame's buffer. Widgets render into an
area; `Layout` splits areas, and `Rect::centered` places a box in the middle.

```rust
use vnce::vnce_core::{Buffer, Color, Rect, Size, Style};
use vnce::vnce_ui::{Block, BorderType, Clear, Gauge, Line, List, ListState, StatefulWidget, Widget};

let mut buffer = Buffer::empty(Size::new(40, 12));
let screen = buffer.area();

// A menu in a rounded box, in the middle of the screen.
let area = screen.centered(24, 7);
let mut menu = ListState::with_selected(Some(0));
Clear.render(area, &mut buffer); // hide what is below (over a game)
List::new(["Start", "Settings", "Quit"])
    .highlight_symbol("▶ ")
    .highlight_style(Style::new().fg(Color::rgb(20, 16, 24)).bg(Color::rgb(255, 210, 110)))
    .block(
        Block::bordered()
            .border_type(BorderType::Rounded)
            .title(Line::from(" MY GAME ").centered()),
    )
    .render(area, &mut buffer, &mut menu);

// In `update`: menu.select_next(3, true) / select_previous(3, true), and
// `menu.selected()` when Enter is pressed.

// A health bar.
Gauge::new().ratio(0.7).label("HP").render(Rect::new(0, 11, 20, 1), &mut buffer);

let row: String = (area.x..area.x + 24)
    .map(|x| buffer.cell(x, area.y + 1).unwrap().symbol().to_owned())
    .collect();
assert!(row.contains("▶ Start"));
```

Other widgets: `Paragraph` (text with wrapping and alignment), `FpsOverlay` (frames per
second in a corner, handy while developing). Text drawn after an image appears over it in
cell modes; with graphics protocols, images cover text, so put menus beside or over a
plain background there.

## Saving

`SaveFile` keeps values between runs in a plain text file (`key = value` per line) in the
user's data folder: `%APPDATA%\my-game` on Windows, `~/Library/Application Support/my-game`
on macOS, `~/.local/share/my-game` elsewhere.

```rust
use vnce::vnce_game::{ActionMap, Binding, SaveFile};
use vnce::vnce_term::KeyCode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Jump,
}

// In a game: `let mut save = SaveFile::open("my-game");`. Here: only in memory.
let mut save = SaveFile::new();
let best: u32 = save.get_or("best", 0); // 0 the first time
save.set("best", best.max(1200));
save.set("music_volume", 60);

let mut controls = ActionMap::new().bind(Action::Jump, ' ');
save.set_bindings("keys.jump", controls.bindings(Action::Jump));
assert_eq!(save.get_str("keys.jump"), Some("Space"));

// Next run: load the bindings back.
if let Some(keys) = save.bindings("keys.jump") {
    controls.unbind(Action::Jump);
    for key in keys {
        controls.add(Action::Jump, key);
    }
}
assert_eq!(controls.bindings(Action::Jump).next(), Some(Binding::Key(KeyCode::Char(' '))));

save.save().unwrap(); // writes the file (nothing for a save in memory)
```

Saving never stops a game: a missing or broken file starts empty, and the file is written
through a temporary file so a crash while saving keeps the old one.

Next: [Sound](07-sound.md).
