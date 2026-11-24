//! Shader quad and mesh renderer tests on a real (or software) GPU. Skipped when no
//! adapter exists.

mod common;

use vnce_gpu::{
    GpuError, Mat4, Mesh, MeshDraw, MeshRenderer, MeshRendererOptions, RenderTarget, ShaderQuad,
    Vertex,
};
use vnce_raster::{Canvas, Rgba};

use common::{assert_close, gpu};

#[test]
fn shader_quad_uv_runs_from_the_top_left() {
    let Some(gpu) = gpu() else { return };
    // Red on the left half, green on the bottom half (blue channel from the frame).
    let shader = ShaderQuad::new(
        &gpu,
        "fn shade(uv: vec2<f32>, time: f32, resolution: vec2<f32>) -> vec4<f32> {
            return vec4<f32>(step(uv.x, 0.5), step(0.5, uv.y), f32(globals.frame) * time, 1.0);
        }",
    )
    .unwrap();
    let mut target = RenderTarget::new(&gpu, 4, 4);
    shader.render(&gpu, &target, 1.0, 1);
    let mut canvas = Canvas::new(0, 0);
    target.read_into(&gpu, &mut canvas).unwrap();
    assert_eq!(canvas.get(0, 0), Some(Rgba::rgb(255, 0, 255)));
    assert_eq!(canvas.get(3, 0), Some(Rgba::rgb(0, 0, 255)));
    assert_eq!(canvas.get(0, 3), Some(Rgba::rgb(255, 255, 255)));
    assert_eq!(canvas.get(3, 3), Some(Rgba::rgb(0, 255, 255)));
}

#[test]
fn shader_errors_are_returned() {
    let Some(gpu) = gpu() else { return };
    let error = ShaderQuad::new(&gpu, "fn shade() {}").unwrap_err();
    assert!(matches!(error, GpuError::Shader(_)), "{error}");
}

#[test]
fn mesh_renderer_draws_with_depth_and_antialiasing() {
    let Some(gpu) = gpu() else { return };
    let red = Rgba::rgb(255, 0, 0);
    let blue = Rgba::rgb(0, 0, 255);
    let background = Rgba::rgb(10, 20, 30);
    // Two full-width quads in clip space: red near (z = 0.2) on the left, blue far
    // (z = 0.8) covering everything. Drawn far-last to check the depth test.
    let quad = |x0: f32, x1: f32, z: f32, color| {
        Mesh::new(
            &gpu,
            &[
                Vertex::new([x0, -1.0, z], color),
                Vertex::new([x1, -1.0, z], color),
                Vertex::new([x1, 1.0, z], color),
                Vertex::new([x0, 1.0, z], color),
            ],
            &[0, 1, 2, 0, 2, 3],
        )
    };
    let near = quad(-1.0, 0.0, 0.2, red);
    let far = quad(-1.0, 1.0, 0.8, blue);
    for msaa in [1, 4] {
        let mut renderer = MeshRenderer::new(
            &gpu,
            MeshRendererOptions {
                msaa,
                ..MeshRendererOptions::default()
            },
        );
        assert!(renderer.samples() <= msaa);
        let mut target = RenderTarget::new(&gpu, 8, 4);
        let draws = [
            MeshDraw {
                mesh: &near,
                transform: Mat4::IDENTITY,
            },
            MeshDraw {
                mesh: &far,
                transform: Mat4::IDENTITY,
            },
        ];
        renderer.render(&gpu, &target, background, &draws);
        let mut canvas = Canvas::new(0, 0);
        target.read_into(&gpu, &mut canvas).unwrap();
        assert_close(canvas.get(1, 1).unwrap(), red);
        assert_close(canvas.get(6, 1).unwrap(), blue);

        // A transform moves the near quad off-screen: the background never shows, the
        // far quad does.
        let moved = [
            MeshDraw {
                mesh: &near,
                transform: Mat4::translation([5.0, 0.0, 0.0]),
            },
            draws[1],
        ];
        renderer.render(&gpu, &target, background, &moved);
        target.read_into(&gpu, &mut canvas).unwrap();
        assert_close(canvas.get(1, 1).unwrap(), blue);

        // Nothing to draw: the background.
        target.resize(&gpu, 3, 3);
        renderer.render(&gpu, &target, background, &[]);
        target.read_into(&gpu, &mut canvas).unwrap();
        assert_close(canvas.get(2, 2).unwrap(), background);
    }
}

#[test]
fn shader_quad_reports_the_real_aspect_ratio() {
    let Some(gpu) = gpu() else { return };
    let mut shader = ShaderQuad::new(
        &gpu,
        "fn shade(uv: vec2<f32>, time: f32, resolution: vec2<f32>) -> vec4<f32> {
            return vec4<f32>(globals.aspect * 0.5, globals.pixel_aspect, 0.0, 1.0);
        }",
    )
    .unwrap();
    // 8×4 pixels that are half as wide as high: the image is square.
    shader.set_pixel_aspect(0.5);
    shader.set_pixel_aspect(-1.0);
    assert!((shader.pixel_aspect() - 0.5).abs() < f32::EPSILON);
    let mut target = RenderTarget::new(&gpu, 8, 4);
    shader.render(&gpu, &target, 0.0, 0);
    let mut canvas = Canvas::new(0, 0);
    target.read_into(&gpu, &mut canvas).unwrap();
    // Linear 0.5 is sRGB 188.
    assert_close(canvas.get(0, 0).unwrap(), Rgba::rgb(188, 188, 0));
}
