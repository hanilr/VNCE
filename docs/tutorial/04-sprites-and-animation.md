# 4. Sprites and animation

## Art in code

Small sprites can be drawn as text: every character is one pixel, the palette gives the
colors, and characters missing from the palette stay transparent.

```rust
use vnce::vnce_raster::{BlitOptions, Canvas, Rgba};

let palette = [
    ('k', Rgba::rgb(30, 24, 40)),
    ('s', Rgba::rgb(255, 210, 170)),
    ('b', Rgba::rgb(60, 120, 220)),
];
let hero = Canvas::from_art(
    &[
        "..kkkk..",
        ".kssssk.",
        ".ksksks.",
        "..kbbk..",
        ".kbbbbk.",
        "..k..k..",
    ],
    &palette,
);
assert_eq!((hero.width(), hero.height()), (8, 6));

// Draw it into the world, mirrored when walking left.
let mut world = Canvas::filled(32, 16, Rgba::rgb(20, 18, 30));
let options = BlitOptions { flip_x: true, ..BlitOptions::default() };
world.blit(&hero, 4, 5, options);
```

`BlitOptions` also has `flip_y`, `opacity` (fading, blinking) and `color_key` (one color
treated as transparent, for images without alpha).

## Sprite sheets

A `SpriteSheet` holds the frames of an animation: from one image cut into a grid
(`SpriteSheet::new(image, frame_width, frame_height)`, for PNG sheets) or from separate
canvases.

```rust
use vnce::vnce_game::SpriteSheet;
use vnce::vnce_raster::{BlitOptions, Canvas, Rgba};

let palette = [('w', Rgba::rgb(240, 240, 255)), ('k', Rgba::rgb(30, 24, 40))];
let wings_up = Canvas::from_art(&["w......w", "ww.kk.ww", ".wwkkww."], &palette);
let wings_down = Canvas::from_art(&["...kk...", ".wwkkww.", "ww....ww"], &palette);
let bat = SpriteSheet::from_frames(&[wings_up, wings_down]);
assert_eq!(bat.frame_count(), 2);

let mut world = Canvas::new(32, 16);
bat.draw(&mut world, 1, 10, 4, BlitOptions::default()); // frame 1 at (10, 4)
```

## Animations

An `Animation` says which frames to show and for how long; an `Animator` plays one. Call
`animator.update(ctx.dt())` in `update` and draw `animator.frame()` in `render`.

```rust
use std::time::Duration;
use vnce::vnce_game::{Animation, Animator, Playback};

let flap = Animation::new([0, 1], Duration::from_millis(150), Playback::Loop);
let mut animator = Animator::new(flap);
assert_eq!(animator.frame(), 0);
animator.update(Duration::from_millis(160));
assert_eq!(animator.frame(), 1);

// Switching animations: `play` only restarts when the animation is different.
let idle = Animation::still(0);
animator.play(&idle);
assert_eq!(animator.frame(), 0);
```

`Playback::Once` stops on the last frame (`animator.is_finished()` tells when),
`Playback::PingPong` plays forwards and backwards, and `Animation::with_durations` gives
every frame its own time.

## Timers and easing

Cooldowns, invincibility after a hit and spawn waves are `Timer`s; `ease` has curves for
animating values.

```rust
use std::time::Duration;
use vnce::vnce_game::{Timer, ease};

let mut invincible = Timer::once(Duration::from_millis(1500));
invincible.update(Duration::from_millis(500));
assert!(!invincible.is_finished());

let mut spawn = Timer::repeating(Duration::from_secs(2));
assert_eq!(spawn.update(Duration::from_millis(4100)), 2); // fired twice

// Slide a menu in over half a second: t goes from 0.0 to 1.0.
let t = 0.5;
let x = ease::lerp(-40.0, 0.0, ease::out_cubic(t));
assert!(x > -20.0); // already more than halfway
```

Next: [Tile maps, camera and collision](05-tilemaps-camera-collision.md).
