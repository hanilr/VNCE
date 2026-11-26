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

A `Tween` does the counting for you: it runs a value (a number, a `[f32; N]` vector, a
pair or an `Rgba` color) from a start to an end over a time, along a curve. `ease` has
quadratic, cubic, sine, back, elastic and bounce curves (`in_`, `out_` and `in_out_`).

```rust
use std::time::Duration;
use vnce::vnce_game::{Tween, ease};
use vnce::vnce_raster::Rgba;

let mut slide = Tween::new(-40.0, 0.0, Duration::from_millis(400)).with_ease(ease::out_back);
slide.update(Duration::from_millis(100)); // in `update`, with the frame's time
let x = slide.value();
assert!(x > -40.0 && !slide.is_finished());

// A camera that eases towards a target that keeps moving: no jumps.
let mut camera = Tween::new([0.0, 0.0], [100.0, 50.0], Duration::from_millis(300));
camera.update(Duration::from_millis(150));
camera.retarget([120.0, 60.0]); // from where it is now, the whole time again

let mut flash = Tween::new(Rgba::WHITE, Rgba::rgb(200, 40, 40), Duration::from_millis(200));
flash.update(Duration::from_millis(200));
assert_eq!(flash.value(), Rgba::rgb(200, 40, 40));
```

## Particles

Sparks, dust, smoke and rain come from an `Emitter`: set it up once, spawn particles in
bursts (or at a steady rate with `emit`), move them every update and draw them through a
projection. Positions are 3D; a 2D game leaves z at zero.

```rust
use std::time::Duration;
use vnce::vnce_game::{Emitter, Rng, Spread};
use vnce::vnce_raster::{BlendMode, Canvas, Rgba};

let mut sparks = Emitter::new()
    .spread(Spread::Circle) // every direction on the screen
    .speed(20.0, 70.0) // pixels per second
    .life(0.4, 0.9) // seconds
    .drag(4.8) // slow down quickly
    .colors([Rgba::rgb(255, 200, 80), Rgba::rgb(255, 120, 40)])
    .end_color(Rgba::rgb(120, 20, 0))
    .fade(0.3); // fade out over the last 0.3 s

let mut dust = Emitter::new()
    .spread(Spread::Arc { angle: -std::f32::consts::FRAC_PI_2, width: 1.0 }) // upwards
    .speed(5.0, 15.0)
    .gravity([0.0, 30.0, 0.0]) // y grows downwards on the screen
    .rate(40.0) // per second, with `emit`
    .blend(BlendMode::Alpha)
    .colors([Rgba::rgb(160, 140, 110)]);

let mut rng = Rng::new(1);
// When a coin is taken:
sparks.burst(&mut rng, [40.0, 30.0, 0.0], 30);
// Every update while the player runs:
let dt = Duration::from_millis(16);
dust.emit(&mut rng, [20.0, 44.0, 0.0], dt);
sparks.update(dt);
dust.update(dt);

// In `render`: x and y are already pixels here; an isometric or 3D game projects them
// (and returns a scale for the size, for example smaller far away).
let mut canvas = Canvas::new(80, 48);
sparks.draw(&mut canvas, |[x, y, _]| Some((x, y, 1.0)));
dust.draw(&mut canvas, |[x, y, _]| Some((x, y, 1.0)));
assert_eq!(sparks.len(), 30);
```

`Spread::Sphere` and `Spread::Cone` fly off in 3D. For a shape the settings do not cover
(a ring of sparks around a gate), make the particles yourself and `spawn` them; the
emitter still moves, fades and draws them.

Next: [Tile maps, camera and collision](05-tilemaps-camera-collision.md).
