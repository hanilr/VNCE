//! Shader quad and mesh renderer tests on a real (or software) GPU. Skipped when no
//! adapter exists.

mod common;

use vnce_gpu::{
    Fog, Gpu, GpuError, Light, Mat4, Mesh, MeshDraw, MeshRenderer, MeshRendererOptions, MeshScene,
    RenderTarget, ShaderQuad, Vertex,
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
fn shader_quad_reads_the_programs_params() {
    let Some(gpu) = gpu() else { return };
    let mut shader = ShaderQuad::new(
        &gpu,
        "fn shade(uv: vec2<f32>, time: f32, resolution: vec2<f32>) -> vec4<f32> {
            return vec4<f32>(globals.params[0].x, globals.params[3].w, 0.0, 1.0);
        }",
    )
    .unwrap();
    let mut params = [[0.0; 4]; 4];
    params[0][0] = 1.0;
    params[3][3] = 1.0;
    shader.set_params(params);
    let mut target = RenderTarget::new(&gpu, 2, 2);
    shader.render(&gpu, &target, 0.0, 0);
    let mut canvas = Canvas::new(0, 0);
    target.read_into(&gpu, &mut canvas).unwrap();
    assert_eq!(canvas.get(1, 1), Some(Rgba::rgb(255, 255, 0)));
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
            MeshDraw::new(&near, Mat4::IDENTITY, Mat4::IDENTITY),
            MeshDraw::new(&far, Mat4::IDENTITY, Mat4::IDENTITY),
        ];
        renderer.render(&gpu, &target, background, &draws);
        let mut canvas = Canvas::new(0, 0);
        target.read_into(&gpu, &mut canvas).unwrap();
        assert_close(canvas.get(1, 1).unwrap(), red);
        assert_close(canvas.get(6, 1).unwrap(), blue);

        // A transform moves the near quad off-screen: the background never shows, the
        // far quad does.
        let moved = [
            MeshDraw::new(&near, Mat4::IDENTITY, Mat4::translation([5.0, 0.0, 0.0])),
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

/// A full-screen quad in clip space at depth `z` (the model is the identity, so world
/// space is clip space), with `normal` on every vertex, or unlit when `normal` is `None`.
fn screen_quad(gpu: &Gpu, z: f32, normal: Option<[f32; 3]>, color: Rgba) -> Mesh {
    let vertex = |x, y| match normal {
        Some(normal) => Vertex::lit([x, y, z], normal, color),
        None => Vertex::new([x, y, z], color),
    };
    Mesh::new(
        gpu,
        &[
            vertex(-1.0, -1.0),
            vertex(1.0, -1.0),
            vertex(1.0, 1.0),
            vertex(-1.0, 1.0),
        ],
        &[0, 1, 2, 0, 2, 3],
    )
}

/// Renders one mesh with `scene` onto a 4×4 target and returns the center pixel.
fn render_center(gpu: &Gpu, mesh: &Mesh, scene: MeshScene, background: Rgba) -> Rgba {
    let mut renderer = MeshRenderer::new(
        gpu,
        MeshRendererOptions {
            msaa: 1,
            ..MeshRendererOptions::default()
        },
    );
    renderer.set_scene(gpu, scene);
    assert_eq!(renderer.scene(), &scene);
    let mut target = RenderTarget::new(gpu, 4, 4);
    renderer.render(
        gpu,
        &target,
        background,
        &[MeshDraw::new(mesh, Mat4::IDENTITY, Mat4::IDENTITY)],
    );
    let mut canvas = Canvas::new(0, 0);
    target.read_into(gpu, &mut canvas).unwrap();
    canvas.get(2, 2).unwrap()
}

#[test]
fn mesh_light_brightens_faces_toward_the_sun() {
    let Some(gpu) = gpu() else { return };
    let white = Rgba::rgb(255, 255, 255);
    let background = Rgba::rgb(0, 0, 0);
    let scene = MeshScene {
        light: Some(Light {
            direction: [0.0, 0.0, -3.0],
            sun_color: [0.8, 0.8, 0.8],
            ambient: [0.2, 0.2, 0.2],
        }),
        ..MeshScene::default()
    };
    // Facing the sun: ambient + sun = 1. Facing away: ambient only, linear 0.2 = sRGB 124.
    let toward = screen_quad(&gpu, 0.5, Some([0.0, 0.0, -1.0]), white);
    assert_close(render_center(&gpu, &toward, scene, background), white);
    let away = screen_quad(&gpu, 0.5, Some([0.0, 0.0, 1.0]), white);
    let dark = render_center(&gpu, &away, scene, background);
    assert!((122..=126).contains(&dark.r), "{dark:?}");
    // Unlit vertices and an unlit scene keep their colors.
    let unlit = screen_quad(&gpu, 0.5, None, white);
    assert_close(render_center(&gpu, &unlit, scene, background), white);
    assert_close(
        render_center(&gpu, &away, MeshScene::default(), background),
        white,
    );
}

#[test]
fn mesh_fog_fades_far_pixels_to_the_fog_color() {
    let Some(gpu) = gpu() else { return };
    let red = Rgba::rgb(255, 0, 0);
    let mist = Rgba::rgb(0, 0, 255);
    let quad = screen_quad(&gpu, 0.5, None, red);
    // The quad is about 10 units from the camera.
    let fogged = |start, end, color| MeshScene {
        fog: Some(Fog { color, start, end }),
        camera: [0.0, 0.0, -9.5],
        ..MeshScene::default()
    };
    let background = Rgba::rgb(0, 0, 0);
    assert_close(
        render_center(&gpu, &quad, fogged(0.0, 5.0, mist), background),
        mist,
    );
    assert_close(
        render_center(&gpu, &quad, fogged(50.0, 100.0, mist), background),
        red,
    );
    let half = render_center(&gpu, &quad, fogged(0.0, 20.0, mist), background);
    assert!(half.r > 100 && half.b > 100, "{half:?}");
    // A transparent fog color fades the mesh out.
    let gone = render_center(
        &gpu,
        &quad,
        fogged(0.0, 5.0, Rgba::TRANSPARENT),
        Rgba::TRANSPARENT,
    );
    assert_eq!(gone.a, 0, "{gone:?}");
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
