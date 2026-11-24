//! Sprites: [`SpriteSheet`] frames and their [`Animation`]s, played by an [`Animator`].

use std::time::Duration;

use vnce_raster::{BlitOptions, Canvas, PixelRect};

/// An image split into equal frames, read left to right, top to bottom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpriteSheet {
    image: Canvas,
    frame_width: u32,
    frame_height: u32,
    margin: u32,
    spacing: u32,
}

impl SpriteSheet {
    /// A sheet of `frame_width` × `frame_height` frames without margin or spacing.
    #[must_use]
    pub fn new(image: Canvas, frame_width: u32, frame_height: u32) -> Self {
        Self {
            image,
            frame_width: frame_width.max(1),
            frame_height: frame_height.max(1),
            margin: 0,
            spacing: 0,
        }
    }

    /// Puts equally sized frames side by side into one sheet (for example frames made with
    /// `Canvas::from_art`). Frames smaller than the first one are padded with
    /// transparency.
    #[must_use]
    pub fn from_frames(frames: &[Canvas]) -> Self {
        let (width, height) = frames
            .first()
            .map_or((1, 1), |f| (f.width().max(1), f.height().max(1)));
        let count = u32::try_from(frames.len()).unwrap_or(u32::MAX).max(1);
        let mut image = Canvas::new(width * count, height);
        let mut x = 0_i32;
        for frame in frames {
            image.blit_region(
                frame,
                PixelRect::new(0, 0, width, height),
                x,
                0,
                BlitOptions::default(),
            );
            x = x.saturating_add_unsigned(width);
        }
        Self::new(image, width, height)
    }

    /// Sets the empty border around the frames and the gap between them, in pixels.
    #[must_use]
    pub fn with_spacing(mut self, margin: u32, spacing: u32) -> Self {
        self.margin = margin;
        self.spacing = spacing;
        self
    }

    /// The whole image.
    #[must_use]
    pub fn image(&self) -> &Canvas {
        &self.image
    }

    /// Size of one frame.
    #[must_use]
    pub fn frame_size(&self) -> (u32, u32) {
        (self.frame_width, self.frame_height)
    }

    /// Frames per row.
    #[must_use]
    pub fn columns(&self) -> u32 {
        fit(
            self.image.width(),
            self.frame_width,
            self.margin,
            self.spacing,
        )
    }

    /// Number of frames.
    #[must_use]
    pub fn frame_count(&self) -> usize {
        let rows = fit(
            self.image.height(),
            self.frame_height,
            self.margin,
            self.spacing,
        );
        self.columns() as usize * rows as usize
    }

    /// Where frame `index` is in the image, or `None` past the last frame.
    #[must_use]
    pub fn frame_rect(&self, index: usize) -> Option<PixelRect> {
        if index >= self.frame_count() {
            return None;
        }
        let columns = self.columns() as usize;
        // Both are below the frame count, which fits in u32.
        let column = u32::try_from(index % columns).ok()?;
        let row = u32::try_from(index / columns).ok()?;
        let x = self.margin + column * (self.frame_width + self.spacing);
        let y = self.margin + row * (self.frame_height + self.spacing);
        Some(PixelRect::new(
            i32::try_from(x).ok()?,
            i32::try_from(y).ok()?,
            self.frame_width,
            self.frame_height,
        ))
    }

    /// Draws frame `index` with its top-left corner at `(x, y)` of `target`. Nothing is
    /// drawn for an index past the last frame.
    pub fn draw(&self, target: &mut Canvas, index: usize, x: i32, y: i32, options: BlitOptions) {
        if let Some(rect) = self.frame_rect(index) {
            target.blit_region(&self.image, rect, x, y, options);
        }
    }
}

/// How many frames of `frame` pixels fit in `total` pixels.
fn fit(total: u32, frame: u32, margin: u32, spacing: u32) -> u32 {
    let usable = total.saturating_sub(margin.saturating_mul(2));
    if usable < frame {
        0
    } else {
        (usable - frame) / (frame + spacing) + 1
    }
}

/// How an [`Animation`] continues after its last frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Playback {
    /// Starts again from the first frame.
    #[default]
    Loop,
    /// Stops on the last frame.
    Once,
    /// Plays backwards to the first frame, then forwards again.
    PingPong,
}

/// A sequence of sprite sheet frames with their durations.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Animation {
    /// Frame indices and how long each is shown, already expanded for ping-pong.
    steps: Vec<(usize, Duration)>,
    playback: Playback,
}

impl Animation {
    /// Frames shown for `frame_time` each.
    pub fn new(
        frames: impl IntoIterator<Item = usize>,
        frame_time: Duration,
        playback: Playback,
    ) -> Self {
        Self::with_durations(frames.into_iter().map(|f| (f, frame_time)), playback)
    }

    /// Frames with their own durations.
    pub fn with_durations(
        steps: impl IntoIterator<Item = (usize, Duration)>,
        playback: Playback,
    ) -> Self {
        let mut steps: Vec<(usize, Duration)> = steps.into_iter().collect();
        if playback == Playback::PingPong && steps.len() > 2 {
            let back: Vec<_> = steps[1..steps.len() - 1].iter().rev().copied().collect();
            steps.extend(back);
        }
        Self { steps, playback }
    }

    /// A single frame shown forever.
    #[must_use]
    pub fn still(frame: usize) -> Self {
        Self::new([frame], Duration::from_secs(1), Playback::Loop)
    }

    /// How the animation continues after its last frame.
    #[must_use]
    pub fn playback(&self) -> Playback {
        self.playback
    }

    /// Duration of one pass (forwards and backwards for ping-pong).
    #[must_use]
    pub fn duration(&self) -> Duration {
        self.steps.iter().map(|&(_, d)| d).sum()
    }

    /// The frame shown `elapsed` after the start.
    #[must_use]
    pub fn frame_at(&self, elapsed: Duration) -> usize {
        let Some(&(last, _)) = self.steps.last() else {
            return 0;
        };
        let total = self.duration().as_nanos();
        if total == 0 {
            return last;
        }
        let mut t = elapsed.as_nanos();
        if self.playback == Playback::Once {
            if t >= total {
                return last;
            }
        } else {
            t %= total;
        }
        for &(frame, duration) in &self.steps {
            if t < duration.as_nanos() {
                return frame;
            }
            t -= duration.as_nanos();
        }
        last
    }

    /// Returns `true` when a [`Playback::Once`] animation has ended at `elapsed`.
    #[must_use]
    pub fn is_finished_at(&self, elapsed: Duration) -> bool {
        self.playback == Playback::Once && elapsed >= self.duration()
    }
}

/// Plays an [`Animation`]: advance it with the time of each update and read the current
/// frame when drawing.
///
/// ```
/// use std::time::Duration;
/// use vnce_game::{Animation, Animator, Playback};
///
/// let walk = Animation::new([4, 5, 6, 7], Duration::from_millis(100), Playback::Loop);
/// let mut animator = Animator::new(walk);
/// animator.update(Duration::from_millis(250));
/// assert_eq!(animator.frame(), 6);
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Animator {
    animation: Animation,
    elapsed: Duration,
    speed_percent: u32,
}

impl Animator {
    /// Starts `animation` from its first frame.
    #[must_use]
    pub fn new(animation: Animation) -> Self {
        Self {
            animation,
            elapsed: Duration::ZERO,
            speed_percent: 100,
        }
    }

    /// Switches to `animation` and restarts it, unless it is already playing (so it can be
    /// called on every update, for example with the animation for the current state).
    pub fn play(&mut self, animation: &Animation) {
        if &self.animation != animation {
            self.animation = animation.clone();
            self.elapsed = Duration::ZERO;
        }
    }

    /// Restarts the current animation.
    pub fn restart(&mut self) {
        self.elapsed = Duration::ZERO;
    }

    /// Sets the playback speed in percent (100 = normal, 200 = twice as fast).
    pub fn set_speed_percent(&mut self, percent: u32) {
        self.speed_percent = percent;
    }

    /// Advances the animation by `dt` (scaled by the speed).
    pub fn update(&mut self, dt: Duration) {
        let scaled = dt.saturating_mul(self.speed_percent) / 100;
        self.elapsed = self.elapsed.saturating_add(scaled);
    }

    /// The sprite sheet frame to draw now.
    #[must_use]
    pub fn frame(&self) -> usize {
        self.animation.frame_at(self.elapsed)
    }

    /// Returns `true` when a [`Playback::Once`] animation has ended.
    #[must_use]
    pub fn is_finished(&self) -> bool {
        self.animation.is_finished_at(self.elapsed)
    }

    /// The animation being played.
    #[must_use]
    pub fn animation(&self) -> &Animation {
        &self.animation
    }
}

#[cfg(test)]
mod tests {
    use vnce_raster::Rgba;

    use super::*;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn sheet_frames_with_margin_and_spacing() {
        // 2 columns × 2 rows of 4×3 frames, margin 1, spacing 2: 1+4+2+4+1 = 12 wide.
        let sheet = SpriteSheet::new(Canvas::new(12, 10), 4, 3).with_spacing(1, 2);
        assert_eq!(sheet.columns(), 2);
        assert_eq!(sheet.frame_count(), 4);
        assert_eq!(sheet.frame_rect(0), Some(PixelRect::new(1, 1, 4, 3)));
        assert_eq!(sheet.frame_rect(3), Some(PixelRect::new(7, 6, 4, 3)));
        assert_eq!(sheet.frame_rect(4), None);
        assert_eq!(SpriteSheet::new(Canvas::new(3, 3), 4, 4).frame_count(), 0);
    }

    #[test]
    fn frames_are_drawn_with_flips() {
        let red = Rgba::rgb(255, 0, 0);
        let blue = Rgba::rgb(0, 0, 255);
        let frames = [
            Canvas::from_art(&["r."], &[('r', red)]),
            Canvas::from_art(&["bb"], &[('b', blue)]),
        ];
        let sheet = SpriteSheet::from_frames(&frames);
        assert_eq!((sheet.frame_count(), sheet.frame_size()), (2, (2, 1)));
        let mut target = Canvas::new(4, 1);
        sheet.draw(&mut target, 0, 0, 0, BlitOptions::default());
        let flipped = BlitOptions {
            flip_x: true,
            ..BlitOptions::default()
        };
        sheet.draw(&mut target, 0, 2, 0, flipped);
        sheet.draw(&mut target, 9, 0, 0, flipped);
        assert_eq!(
            target.pixels(),
            [red, Rgba::TRANSPARENT, Rgba::TRANSPARENT, red]
        );
    }

    #[test]
    fn loop_once_and_ping_pong() {
        let looping = Animation::new([1, 2, 3], 10 * MS, Playback::Loop);
        let frames: Vec<usize> = (0..7).map(|i| looping.frame_at(i * 10 * MS)).collect();
        assert_eq!(frames, [1, 2, 3, 1, 2, 3, 1]);
        assert!(!looping.is_finished_at(Duration::from_secs(9)));

        let once = Animation::new([1, 2, 3], 10 * MS, Playback::Once);
        assert_eq!(once.frame_at(29 * MS), 3);
        assert_eq!(once.frame_at(Duration::from_secs(9)), 3);
        assert!(!once.is_finished_at(29 * MS) && once.is_finished_at(30 * MS));

        let ping = Animation::new([1, 2, 3, 4], 10 * MS, Playback::PingPong);
        let frames: Vec<usize> = (0..8).map(|i| ping.frame_at(i * 10 * MS)).collect();
        assert_eq!(frames, [1, 2, 3, 4, 3, 2, 1, 2]);
        assert_eq!(ping.duration(), 60 * MS);
    }

    #[test]
    fn per_frame_durations_and_edge_cases() {
        let blink = Animation::with_durations([(0, 90 * MS), (1, 10 * MS)], Playback::Loop);
        assert_eq!(blink.frame_at(89 * MS), 0);
        assert_eq!(blink.frame_at(95 * MS), 1);
        assert_eq!(Animation::new([], 10 * MS, Playback::Loop).frame_at(MS), 0);
        assert_eq!(
            Animation::new([7, 8], Duration::ZERO, Playback::Loop).frame_at(MS),
            8
        );
        assert_eq!(Animation::still(5).frame_at(Duration::from_secs(99)), 5);
    }

    #[test]
    fn animator_switches_only_when_needed() {
        let idle = Animation::new([0, 1], 100 * MS, Playback::Loop);
        let jump = Animation::new([5, 6], 50 * MS, Playback::Once);
        let mut animator = Animator::new(idle.clone());
        animator.update(150 * MS);
        assert_eq!(animator.frame(), 1);
        // Playing the same animation again does not restart it.
        animator.play(&idle);
        assert_eq!(animator.frame(), 1);
        animator.play(&jump);
        assert_eq!(animator.frame(), 5);
        animator.set_speed_percent(200);
        animator.update(30 * MS);
        assert_eq!(animator.frame(), 6);
        animator.update(30 * MS);
        assert!(animator.is_finished());
        animator.restart();
        assert!(!animator.is_finished());
        assert_eq!(animator.animation(), &jump);
    }
}
