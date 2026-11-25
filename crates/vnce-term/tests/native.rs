//! Frames given directly to a backend that draws them itself (a native window).

use std::io;
use std::sync::Arc;

use vnce_core::{Rect, Size, Style};
use vnce_raster::{Canvas, Rgba};
use vnce_term::{
    Backend, Capabilities, ExternalImage, FrameView, GraphicsMode, ImageView, Terminal,
    TerminalInfo, TerminalOptions,
};

/// What one presented frame contained.
#[derive(Debug, Default, Clone, PartialEq)]
struct Seen {
    text: String,
    /// Pixel images: area, width, changed.
    pixels: Vec<(Rect, u32, bool)>,
    /// External images: area, the handle's number.
    external: Vec<(Rect, u32)>,
    full_redraw: bool,
}

/// A backend that records presented frames and never receives escape sequences.
struct Window {
    frames: Vec<Seen>,
    written: usize,
}

impl Backend for Window {
    fn size(&self) -> io::Result<Size> {
        Ok(Size::new(20, 6))
    }

    fn write_all(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.written += bytes.len();
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn enter(&mut self, _options: &TerminalOptions) -> io::Result<()> {
        Ok(())
    }

    fn leave(&mut self) -> io::Result<()> {
        Ok(())
    }

    fn terminal_info(&self) -> TerminalInfo {
        TerminalInfo {
            native_window: true,
            cell_pixels: Some((8, 16)),
            ..TerminalInfo::default()
        }
    }

    fn present(&mut self, frame: &FrameView<'_>) -> Option<io::Result<()>> {
        let mut seen = Seen {
            full_redraw: frame.full_redraw,
            ..Seen::default()
        };
        for x in 0..frame.buffer.size().width {
            if let Some(cell) = frame.buffer.cell(x, 0) {
                seen.text.push_str(cell.symbol());
            }
        }
        for image in &frame.images {
            match *image {
                ImageView::Pixels {
                    area,
                    canvas,
                    changed,
                } => seen.pixels.push((area, canvas.width(), changed)),
                ImageView::External { area, image } => seen
                    .external
                    .push((area, *image.handle().downcast_ref::<u32>().unwrap())),
            }
        }
        assert_eq!(frame.cell_pixels, (8, 16));
        self.frames.push(seen);
        Some(Ok(()))
    }
}

#[test]
fn a_window_gets_frames_without_escape_sequences() {
    let backend = Window {
        frames: Vec::new(),
        written: 0,
    };
    let mut terminal = Terminal::new(backend, Capabilities::default()).unwrap();
    terminal.enter_mode(TerminalOptions::default()).unwrap();
    assert_eq!(terminal.graphics_mode(), GraphicsMode::Native);
    let picture = Canvas::filled(32, 16, Rgba::rgb(10, 200, 30));
    let area = Rect::new(2, 1, 4, 2);
    for frame in 0..3_u32 {
        terminal
            .draw(|f| {
                f.buffer_mut().set_string(0, 0, "hello", Style::new());
                // Native images are shown at the cells' real pixels.
                assert_eq!(f.image_pixel_size(area), (32, 32));
                assert!((f.image_pixel_aspect() - 1.0).abs() < f32::EPSILON);
                f.draw_image(area, &picture);
                assert!(f.accepts_textures());
                if frame > 0 {
                    let texture = ExternalImage::new(64, 32, Arc::new(frame));
                    assert!(f.draw_texture(Rect::new(10, 2, 8, 2), texture));
                }
            })
            .unwrap();
    }
    let backend = terminal.backend();
    assert_eq!(backend.written, 0, "no escape sequences");
    let frames = &backend.frames;
    assert_eq!(frames.len(), 3);
    assert!(frames[0].text.starts_with("hello"));
    assert!(frames[0].full_redraw && !frames[1].full_redraw);
    // The same picture is uploaded once.
    assert_eq!(frames[0].pixels, vec![(area, 32, true)]);
    assert_eq!(frames[1].pixels, vec![(area, 32, false)]);
    assert_eq!(frames[2].external, vec![(Rect::new(10, 2, 8, 2), 2)]);
}

#[test]
fn textures_are_refused_outside_a_window() {
    let backend = vnce_term::TestBackend::new(20, 6);
    let mut terminal = Terminal::new(backend, Capabilities::default()).unwrap();
    terminal
        .draw(|f| {
            assert!(!f.accepts_textures());
            let texture = ExternalImage::new(4, 4, Arc::new(0_u32));
            assert!(!f.draw_texture(Rect::new(0, 0, 2, 2), texture));
        })
        .unwrap();
}

#[test]
fn the_window_mode_has_a_name_and_is_only_offered_in_a_window() {
    assert_eq!("window".parse::<GraphicsMode>(), Ok(GraphicsMode::Native));
    assert_eq!(GraphicsMode::Native.to_string(), "window");
    let window = TerminalInfo {
        native_window: true,
        ..TerminalInfo::default()
    };
    let capabilities = Capabilities::default();
    assert_eq!(
        GraphicsMode::available(&window, &capabilities)[0],
        GraphicsMode::Native
    );
    assert!(
        !GraphicsMode::available(&TerminalInfo::default(), &capabilities)
            .contains(&GraphicsMode::Native)
    );
    // `VNCE_GRAPHICS=window` in a terminal falls back.
    let mode = GraphicsMode::from_env(
        |name| (name == "VNCE_GRAPHICS").then(|| "window".to_owned()),
        &TerminalInfo::default(),
        &capabilities,
    );
    assert_ne!(mode, GraphicsMode::Native);
}
