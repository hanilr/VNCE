# Tutorial

This tutorial builds up a small game step by step, from an empty screen to sprites,
scrolling worlds, menus and sound. Every chapter is short and every code block compiles:
the blocks are tested together with the engine (`cargo test --doc -p vnce`).

You need Rust (1.87 or newer) and a terminal. Windows Terminal, WezTerm, Kitty, iTerm2,
foot and most Linux terminals work; the engine picks the best way to draw pixels in each.

## Chapters

1. [First window](01-first-window.md): the `Game` trait, `run`, drawing text.
2. [Game loop and input](02-game-loop-and-input.md): fixed updates, held keys, actions.
3. [Pixel graphics](03-pixel-graphics.md): the canvas, shapes, text and display methods.
4. [Sprites and animation](04-sprites-and-animation.md): art in code, sprite sheets,
   animators, tweens, particles.
5. [Tile maps, camera and collision](05-tilemaps-camera-collision.md): worlds bigger than
   the screen, walls, many objects, random levels, path finding.
6. [Scenes, UI and saving](06-scenes-and-ui.md): title, pause and settings screens,
   widgets, text fields, the mouse, save files.
7. [Sound](07-sound.md): effects and music generated in code.
8. [Performance tips](08-performance.md): display methods, frame rates and release builds.

## Setting up

Create a new project and add the engine:

```sh
cargo new my-game
cd my-game
cargo add vnce
```

`vnce` re-exports every part of the engine: `vnce::vnce_runtime` (the game loop),
`vnce::vnce_raster` (pixels), `vnce::vnce_game` (game building blocks), `vnce::vnce_ui`
(widgets) and, with features, `vnce::vnce_audio` (`audio`), `vnce::vnce_gpu` (`gpu`) and
`vnce::vnce_window` (`window`, a native window for releases, see
[Shipping a game](../SHIPPING-GAMES.md)).

The finished examples in the repository show everything together:

```sh
cargo run --release --example stars       # chapter 2 as a game
cargo run --release --example platformer  # sprites, tiles, camera, scenes
cargo run --release --example lantern     # the whole engine
```
