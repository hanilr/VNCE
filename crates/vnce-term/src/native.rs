//! Frames for backends that draw them themselves, such as a native window: the finished
//! cell buffer and the images, without escape sequences (see [`Backend::present`]).
//!
//! [`Backend::present`]: crate::Backend::present

use std::any::Any;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use vnce_core::{Buffer, Rect};
use vnce_raster::Canvas;

/// A request from the game to a native window (see [`Backend::window`]). Terminals ignore
/// every request.
///
/// New kinds of requests may be added in later versions, so a `match` needs a `_` arm.
///
/// [`Backend::window`]: crate::Backend::window
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum WindowRequest {
    /// Borderless fullscreen on (`true`) or off.
    Fullscreen(bool),
    /// Shows or hides the mouse cursor over the window.
    CursorVisible(bool),
    /// Changes the title bar.
    Title(String),
    /// Waits for the screen's refresh before showing a frame (no tearing) or not.
    Vsync(bool),
    /// Scales pictures smoothly (`true`) or with sharp pixels.
    SmoothScaling(bool),
    /// Rumbles the connected gamepads at `strength` (`0.0..=1.0`) for `duration`.
    Rumble {
        /// How strongly, from `0.0` to `1.0`.
        strength: f32,
        /// How long.
        duration: Duration,
    },
}

/// What a native window is doing now, for settings screens (see
/// [`Backend::window_state`]).
///
/// [`Backend::window_state`]: crate::Backend::window_state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent switches of a settings screen"
)]
pub struct WindowState {
    /// Borderless fullscreen (F11 and Alt+Enter switch it too).
    pub fullscreen: bool,
    /// The mouse cursor is shown over the window.
    pub cursor_visible: bool,
    /// Frames wait for the screen's refresh.
    pub vsync: bool,
    /// Pictures are scaled smoothly (else with sharp pixels).
    pub smooth_scaling: bool,
}

impl WindowState {
    /// The state after `request`.
    #[must_use]
    pub fn with(mut self, request: &WindowRequest) -> Self {
        match request {
            WindowRequest::Fullscreen(on) => self.fullscreen = *on,
            WindowRequest::CursorVisible(on) => self.cursor_visible = *on,
            WindowRequest::Vsync(on) => self.vsync = *on,
            WindowRequest::SmoothScaling(on) => self.smooth_scaling = *on,
            WindowRequest::Title(_) | WindowRequest::Rumble { .. } => {}
        }
        self
    }
}

/// A picture that already lives where the backend draws, for example a GPU texture,
/// shown with [`Frame::draw_texture`](crate::Frame::draw_texture) without copying its
/// pixels. The backend recognizes the handle's type (a window backend, for example, a
/// texture of its own GPU device) and ignores handles it does not know.
#[derive(Clone)]
pub struct ExternalImage {
    width: u32,
    height: u32,
    handle: Arc<dyn Any + Send + Sync>,
    premultiplied: bool,
}

impl ExternalImage {
    /// An image of `width` × `height` pixels behind `handle`.
    #[must_use]
    pub fn new(width: u32, height: u32, handle: Arc<dyn Any + Send + Sync>) -> Self {
        Self {
            width,
            height,
            handle,
            premultiplied: false,
        }
    }

    /// Marks the colors as premultiplied by their alpha (as GPU drawing on a transparent
    /// background leaves them), so they are laid over what is below correctly.
    #[must_use]
    pub fn premultiplied(mut self, premultiplied: bool) -> Self {
        self.premultiplied = premultiplied;
        self
    }

    /// Whether the colors are premultiplied by their alpha.
    #[must_use]
    pub fn is_premultiplied(&self) -> bool {
        self.premultiplied
    }

    /// Size in pixels.
    #[must_use]
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// The handle, to be downcast by the backend.
    #[must_use]
    pub fn handle(&self) -> &(dyn Any + Send + Sync) {
        &*self.handle
    }
}

impl fmt::Debug for ExternalImage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ExternalImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .field("premultiplied", &self.premultiplied)
            .finish_non_exhaustive()
    }
}

/// An image of the frame being drawn in [`GraphicsMode::Native`](crate::GraphicsMode).
#[derive(Debug)]
pub(crate) enum NativeRequest {
    /// Pixels; `None` when the identical image is already shown in this area.
    Pixels {
        area: Rect,
        canvas: Option<Canvas>,
    },
    External {
        area: Rect,
        image: ExternalImage,
    },
}

/// An image shown in the last frame (kept to find unchanged images).
#[derive(Debug)]
pub(crate) enum NativeImage {
    Pixels {
        area: Rect,
        canvas: Canvas,
        changed: bool,
    },
    External {
        area: Rect,
        image: ExternalImage,
    },
}

impl NativeImage {
    /// The images of a new frame from its `requests`, taking the pixels of unchanged
    /// images from `shown` (the last frame). Canvases of images that left are given to
    /// `recycle`.
    pub(crate) fn update(
        shown: &mut Vec<Self>,
        requests: Vec<NativeRequest>,
        mut recycle: impl FnMut(Canvas),
    ) {
        let mut old = std::mem::take(shown);
        for request in requests {
            match request {
                NativeRequest::Pixels {
                    area,
                    canvas: Some(canvas),
                } => shown.push(Self::Pixels {
                    area,
                    canvas,
                    changed: true,
                }),
                NativeRequest::Pixels { area, canvas: None } => {
                    let index = old.iter().position(
                        |image| matches!(image, Self::Pixels { area: a, .. } if *a == area),
                    );
                    if let Some(Self::Pixels { canvas, .. }) = index.map(|i| old.swap_remove(i)) {
                        shown.push(Self::Pixels {
                            area,
                            canvas,
                            changed: false,
                        });
                    }
                }
                NativeRequest::External { area, image } => {
                    shown.push(Self::External { area, image });
                }
            }
        }
        for image in old {
            if let Self::Pixels { canvas, .. } = image {
                recycle(canvas);
            }
        }
    }

    pub(crate) fn view(&self) -> ImageView<'_> {
        match self {
            Self::Pixels {
                area,
                canvas,
                changed,
            } => ImageView::Pixels {
                area: *area,
                canvas,
                changed: *changed,
            },
            Self::External { area, image } => ImageView::External { area: *area, image },
        }
    }
}

/// An image of a [`FrameView`], in drawing order (later ones on top).
#[derive(Debug, Clone, Copy)]
pub enum ImageView<'a> {
    /// Pixels drawn with [`Frame::draw_image`](crate::Frame::draw_image), to be scaled
    /// to fill `area` (in cells). `changed` is `false` when the same pixels were shown in
    /// the same area in the last frame, so they need not be uploaded again.
    Pixels {
        /// Where, in cells.
        area: Rect,
        /// The pixels.
        canvas: &'a Canvas,
        /// Whether they differ from the last frame's in this area.
        changed: bool,
    },
    /// A picture drawn with [`Frame::draw_texture`](crate::Frame::draw_texture).
    External {
        /// Where, in cells.
        area: Rect,
        /// The picture.
        image: &'a ExternalImage,
    },
}

/// A finished frame for [`Backend::present`](crate::Backend::present).
#[derive(Debug)]
pub struct FrameView<'a> {
    /// Every cell of the frame.
    pub buffer: &'a Buffer,
    /// The images over the cells, in drawing order.
    pub images: Vec<ImageView<'a>>,
    /// The first frame, or the first after a resize, a mode change or a clear.
    pub full_redraw: bool,
    /// Size of one cell in pixels, as the frame was laid out.
    pub cell_pixels: (u16, u16),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_images_keep_their_pixels_and_left_ones_are_recycled() {
        let area = |x| Rect::new(x, 0, 2, 2);
        let mut shown = Vec::new();
        NativeImage::update(
            &mut shown,
            vec![
                NativeRequest::Pixels {
                    area: area(0),
                    canvas: Some(Canvas::new(4, 4)),
                },
                NativeRequest::Pixels {
                    area: area(5),
                    canvas: Some(Canvas::new(2, 2)),
                },
            ],
            |_| panic!("nothing left yet"),
        );
        assert!(matches!(
            shown[0].view(),
            ImageView::Pixels { changed: true, .. }
        ));
        let mut recycled = 0;
        let texture = ExternalImage::new(8, 8, Arc::new(7_u32));
        NativeImage::update(
            &mut shown,
            vec![
                NativeRequest::Pixels {
                    area: area(0),
                    canvas: None,
                },
                NativeRequest::External {
                    area: area(9),
                    image: texture,
                },
            ],
            |_| recycled += 1,
        );
        assert_eq!(recycled, 1);
        assert_eq!(shown.len(), 2);
        match shown[0].view() {
            ImageView::Pixels {
                canvas, changed, ..
            } => assert!(!changed && canvas.width() == 4),
            ImageView::External { .. } => panic!("pixels expected"),
        }
        match shown[1].view() {
            ImageView::External { image, .. } => {
                assert_eq!(image.size(), (8, 8));
                assert_eq!(image.handle().downcast_ref::<u32>(), Some(&7));
            }
            ImageView::Pixels { .. } => panic!("external expected"),
        }
    }
}
