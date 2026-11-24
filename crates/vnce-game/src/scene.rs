//! Screens of a game (title, play, pause, game over): [`Scene`]s on a [`SceneStack`].

use vnce_runtime::{Context, Game};
use vnce_term::{Event, Frame};

/// What the [`SceneStack`] does after a scene handled an event or an update.
pub enum Transition<D> {
    /// Stay on this scene.
    None,
    /// Put a scene on top (for example a pause menu); this one waits below.
    Push(Box<dyn Scene<D>>),
    /// Remove this scene and go back to the one below. Popping the last scene quits.
    Pop,
    /// Replace this scene (for example title → game).
    Switch(Box<dyn Scene<D>>),
    /// Quit the game.
    Quit,
}

impl<D> Transition<D> {
    /// [`Transition::Push`] without writing `Box::new`.
    pub fn push(scene: impl Scene<D> + 'static) -> Self {
        Self::Push(Box::new(scene))
    }

    /// [`Transition::Switch`] without writing `Box::new`.
    pub fn switch(scene: impl Scene<D> + 'static) -> Self {
        Self::Switch(Box::new(scene))
    }
}

impl<D> std::fmt::Debug for Transition<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::None => "None",
            Self::Push(_) => "Push",
            Self::Pop => "Pop",
            Self::Switch(_) => "Switch",
            Self::Quit => "Quit",
        })
    }
}

/// One screen of a game. Like [`Game`], but every scene sees the data shared by all
/// scenes (`D`, for example the score or the settings) and returns a [`Transition`].
pub trait Scene<D = ()> {
    /// Called when the scene is put on the stack (pushed, switched to, or the first one).
    fn start(&mut self, _data: &mut D, _ctx: &mut Context) {}

    /// Called when the scene above this one was popped.
    fn resume(&mut self, _data: &mut D, _ctx: &mut Context) {}

    /// Called for every input event while this scene is on top.
    fn event(&mut self, _data: &mut D, _ctx: &mut Context, _event: &Event) -> Transition<D> {
        Transition::None
    }

    /// Called at the fixed update rate while this scene is on top.
    fn update(&mut self, data: &mut D, ctx: &mut Context) -> Transition<D>;

    /// Draws the scene.
    fn render(&mut self, data: &D, ctx: &Context, frame: &mut Frame<'_>);

    /// Returns `true` for scenes drawn over the ones below (a pause menu over the game):
    /// the scenes below are drawn first, but they are not updated.
    fn is_overlay(&self) -> bool {
        false
    }
}

/// A stack of [`Scene`]s that runs as a [`Game`]: the top scene receives events and
/// updates; it and the scenes below it that it covers only partly are drawn.
///
/// ```no_run
/// use vnce_game::{Scene, SceneStack, Transition};
/// use vnce_runtime::{Context, RunConfig};
/// use vnce_term::Frame;
///
/// struct Title;
///
/// impl Scene<u32> for Title {
///     fn update(&mut self, _best: &mut u32, _ctx: &mut Context) -> Transition<u32> {
///         Transition::None
///     }
///
///     fn render(&mut self, best: &u32, _ctx: &Context, frame: &mut Frame<'_>) {
///         let text = format!("Best score: {best}");
///         frame.buffer_mut().set_string(0, 0, &text, Default::default());
///     }
/// }
///
/// let mut game = SceneStack::new(Title, 0_u32);
/// vnce_runtime::run(&mut game, RunConfig::default())?;
/// # Ok::<(), vnce_runtime::RunError>(())
/// ```
pub struct SceneStack<D> {
    scenes: Vec<Box<dyn Scene<D>>>,
    /// The first scene until `init` starts it.
    pending: Option<Box<dyn Scene<D>>>,
    data: D,
}

impl<D> SceneStack<D> {
    /// A stack starting with `first`, sharing `data` between scenes.
    pub fn new(first: impl Scene<D> + 'static, data: D) -> Self {
        Self {
            scenes: Vec::new(),
            pending: Some(Box::new(first)),
            data,
        }
    }

    /// The data shared by the scenes.
    pub fn data(&self) -> &D {
        &self.data
    }

    /// Mutable access to the data shared by the scenes.
    pub fn data_mut(&mut self) -> &mut D {
        &mut self.data
    }

    /// Number of scenes on the stack.
    #[must_use]
    pub fn len(&self) -> usize {
        self.scenes.len() + usize::from(self.pending.is_some())
    }

    /// Returns `true` when no scene is left (the game is quitting).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn apply(&mut self, transition: Transition<D>, ctx: &mut Context) {
        match transition {
            Transition::None => {}
            Transition::Push(mut scene) => {
                scene.start(&mut self.data, ctx);
                self.scenes.push(scene);
            }
            Transition::Pop => {
                self.scenes.pop();
                match self.scenes.last_mut() {
                    Some(scene) => scene.resume(&mut self.data, ctx),
                    None => ctx.quit(),
                }
            }
            Transition::Switch(mut scene) => {
                self.scenes.pop();
                scene.start(&mut self.data, ctx);
                self.scenes.push(scene);
            }
            Transition::Quit => ctx.quit(),
        }
    }
}

impl<D> std::fmt::Debug for SceneStack<D> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SceneStack")
            .field("scenes", &self.len())
            .finish_non_exhaustive()
    }
}

impl<D> Game for SceneStack<D> {
    fn init(&mut self, ctx: &mut Context) {
        if let Some(first) = self.pending.take() {
            self.apply(Transition::Push(first), ctx);
        }
    }

    fn event(&mut self, ctx: &mut Context, event: &Event) {
        if let Some(scene) = self.scenes.last_mut() {
            let transition = scene.event(&mut self.data, ctx, event);
            self.apply(transition, ctx);
        }
    }

    fn update(&mut self, ctx: &mut Context) {
        if let Some(scene) = self.scenes.last_mut() {
            let transition = scene.update(&mut self.data, ctx);
            self.apply(transition, ctx);
        }
    }

    fn render(&mut self, ctx: &Context, frame: &mut Frame<'_>) {
        // Draw from the lowest scene that is visible: below every overlay on top.
        let mut first = self.scenes.len().saturating_sub(1);
        while first > 0 && self.scenes[first].is_overlay() {
            first -= 1;
        }
        for scene in &mut self.scenes[first..] {
            scene.render(&self.data, ctx, frame);
        }
    }
}
