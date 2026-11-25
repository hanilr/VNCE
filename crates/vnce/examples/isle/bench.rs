//! `--bench`: plays the island without a screen, walking a fixed route and turning the
//! camera, and prints the time and output per frame for several display methods.

use vnce::vnce_runtime::{Context, Game};
use vnce::vnce_term::{Event, Frame};

/// Runs a game for a fixed number of frames, then quits.
pub struct Limited<G> {
    pub game: G,
    pub frames: u64,
}

impl<G: Game> Game for Limited<G> {
    fn init(&mut self, ctx: &mut Context) {
        self.game.init(ctx);
    }

    fn event(&mut self, ctx: &mut Context, event: &Event) {
        self.game.event(ctx, event);
    }

    fn update(&mut self, ctx: &mut Context) {
        self.game.update(ctx);
    }

    fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>) {
        self.game.render(ctx, frame);
        if ctx.frame_count() + 1 >= self.frames {
            ctx.quit();
        }
    }
}
