# 8. Performance tips

A terminal is not a graphics card: every frame travels as text through the terminal's
input. These habits keep games smooth.

## Build in release mode

Debug builds of pixel code are many times slower. Play and measure with:

```sh
cargo run --release
```

To keep debug builds usable while developing, optimize the dependencies even in debug
builds (in your `Cargo.toml`):

```toml
[profile.dev.package."*"]
opt-level = 3
```

## Pick the right display method

- **Kitty** sends compressed real pixels and is the fastest protocol.
- **Cells** (`halfblock`, `quadrant`, …) send only the cells that changed. Half blocks are
  cheap and keep every pixel's color; they are a good default for pixel art.
- **Sixel** and **iTerm2** send whole images. The engine skips images that did not change
  and waits a few frames when the terminal falls behind, but large, constantly changing
  images (fast scrolling) are limited by how quickly the terminal decodes them.

Let players switch (`GraphicsMode::available`, `ctx.set_graphics_mode`) and remember their
choice in the save file, as chapter 6 shows.

## Draw fewer pixels

- Render the world at a small fixed size (for example 160 × 90 or a whole fraction of
  `frame.image_pixel_size(area)`) and let `draw_image` scale it up.
- Draw only what the camera sees: `TileMap::draw` already skips hidden tiles; do the same
  for your objects with `camera.visible(alpha)`.
- Keep big canvases between frames and `clear` them instead of creating new ones.

```rust
use vnce::vnce_raster::{Canvas, Rgba};

struct Renderer {
    view: Canvas,
}

impl Renderer {
    fn frame(&mut self, width: u32, height: u32) -> &mut Canvas {
        if (self.view.width(), self.view.height()) != (width, height) {
            self.view.resize(width, height); // only after a terminal resize
        }
        self.view.clear(Rgba::BLACK);
        &mut self.view
    }
}

let mut renderer = Renderer { view: Canvas::new(0, 0) };
let canvas = renderer.frame(160, 90);
canvas.fill_rect(10, 10, 4, 4, Rgba::WHITE);
assert_eq!(canvas.width(), 160);
```

## Choose a frame rate

Updates run at a fixed rate whatever the frame rate is, so the frame limit only changes
smoothness. 60 FPS is the default; 30 is plenty for slow games and halves the output;
offer a setting (`ctx.set_fps_limit`). Measure with `ctx.fps()` and `ctx.ups()`, or show
them while developing:

```rust
use vnce::vnce_core::{Buffer, Size};
use vnce::vnce_ui::{FpsOverlay, Widget};

let mut buffer = Buffer::empty(Size::new(40, 10));
let area = buffer.area();
FpsOverlay::new(59.8).ups(60.0).render(area, &mut buffer); // in `render`
```

## Heavy effects on the GPU

With the `gpu` feature, `vnce::vnce_gpu` renders shaders and 3D meshes with `wgpu` (Vulkan,
Metal, DirectX 12 or OpenGL) into a canvas that is then shown like any other. See the
`gpu_shader` and `gpu_cube` examples.

## Over SSH and in tmux

Over slow connections prefer cells with a low frame rate. Inside tmux the engine uses
Sixel when tmux supports it and Kitty images through passthrough when the outer terminal
answers; set `VNCE_GRAPHICS=halfblock` when in doubt.

That's the tour. The `lantern` example puts everything together: generated caves, a
camera, sprites, a light effect, scenes, settings, saved controls and sound.
