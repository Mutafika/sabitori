//! sabitori-reel `three-d`: a real 3D stage under the 2D overlay.
//!
//! The backdrop is `seimei` — a PBR renderer with its own lights and materials
//! — running on the *same* offscreen wgpu device the reel already uses, so the
//! 3D pass writes straight into the frame texture and the Element tree is
//! composited on top of it. No readback, no image round-trip.
//!
//! The subject is the obvious one for EnchuDB: a glass 円柱 with data stacked
//! inside it, on a slow turntable. Everything below is still a pure function of
//! the frame number, so scrubbing backwards lands on the identical image.
//!
//!   cargo run -p sabitori-reel --example stage3d --features three-d --release
//!
//! Output: `$TMPDIR/sabitori-reel-stage3d/frame_00000.png` …

use sabitori_reel::three::{
    procedural, Camera, InstanceData, Light, Point3, Renderer, Stage, Vec3D, MODEL_GLASS,
    MODEL_STANDARD,
};
use sabitori_reel::{
    div, interpolate_eased, spring_with, text, Color, Easing, Element, FrameCtx, Px, Reel, Scene,
    Seq, Spring,
};

const FPS: u32 = 30;
const SECS: u32 = 5;
const CHIPS: usize = 7;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}

struct EnchuStage;

impl Scene for EnchuStage {
    // ---- 3D -------------------------------------------------------------

    fn setup_stage(&self, r: &mut Renderer) {
        // The cylinder mesh is Y-up and centred on the origin, so a "chip" is
        // just the same primitive squashed flat.
        r.add_mesh("shell", &procedural::cylinder(1.25, 3.0, 96), None);
        r.add_mesh("chip", &procedural::cylinder(1.0, 0.12, 64), None);
        // Big enough that its far edge never enters frame — a visible horizon
        // line reads as "a plane", not as a studio floor.
        r.add_mesh("floor", &procedural::plane(4000.0, 4000.0, 1, 1), None);

        // Three-point light: cool key from front-left, warm rim from behind.
        r.update_lights(
            [0.035, 0.042, 0.06],
            &[
                Light::directional([-0.5, -0.75, -0.45], [0.72, 0.86, 1.0], 1.6),
                // Rim lights sit *behind* the subject so they graze the glass
                // edge instead of pooling on the floor in front of the title.
                Light::point([3.2, 1.4, -3.2], [1.0, 0.62, 0.38], 2.2),
                Light::point([-3.0, 1.1, -2.6], [0.30, 0.92, 0.82], 2.0),
            ],
            false,
        );
    }

    fn stage(&self, ctx: FrameCtx) -> Option<Stage> {
        let t = ctx.t();

        // Turntable: a slow constant orbit, so no frame is special. The dolly
        // runs the whole duration — a push that stops halfway leaves the back
        // half of the shot dead.
        let yaw = t * 0.42;
        let (s, c) = yaw.sin_cos();
        let dist = interpolate_eased(
            t,
            [0.0, SECS as f32],
            [9.4, 6.4],
            Easing::EaseOutCubic,
        );
        let mut camera = Camera::new();
        camera.position = Point3::new((s * dist) as f64, 2.15, (c * dist) as f64);
        camera.target = Point3::new(0.0, 0.15, 0.0);
        camera.up = Vec3D::new(0.0, 1.0, 0.0);
        camera.fov = 38.0;
        // seimei's defaults are CAD-scale (near 10, far 1e6); this scene is
        // metres, so the near plane has to come in or the subject is clipped.
        camera.near = 0.05;
        camera.far = 9000.0;
        camera.set_aspect(ctx.width as u32, ctx.height as u32);

        let mut instances = vec![(
            "floor".to_string(),
            InstanceData {
                model: Stage::at([0.0, -1.62, 0.0], 1.0),
                // Near-black and *rough*: a smooth floor catches the key light
                // as one broad specular sheet and turns the whole lower frame
                // into a grey wall. Rough kills the sheen and keeps it a floor.
                color: [0.012, 0.014, 0.019, 1.0],
                material: [0.0, 0.92, 0.0, 0.0],
                model_id: MODEL_STANDARD,
            },
        )];

        // Data chips rise into the cylinder one after another and settle with a
        // spring — the "ingest" beat, but in 3D this time. `since` (not
        // `enter`) so each chip keeps its local clock running after the beat
        // and stays stacked instead of vanishing.
        for i in 0..CHIPS {
            let beat = Seq::new(0.35 + i as f32 * 0.22, 1.1);
            let Some(local) = beat.since(ctx) else {
                continue;
            };
            let p = spring_with(local.t(), FPS, Spring::default());
            let rest_y = -1.28 + i as f32 * 0.4;
            let y = -3.4 + (rest_y + 3.4) * p;
            let hue = i as f32 / (CHIPS - 1) as f32;
            instances.push((
                "chip".to_string(),
                InstanceData {
                    model: Stage::spun([0.0, y, 0.0], 1.0, yaw * 0.5 + i as f32 * 0.4),
                    // Saturated, and barely emissive — pushing emissive up just
                    // clips every chip to white behind the glass.
                    color: [0.06 + hue * 0.46, 0.72 - hue * 0.34, 0.66 + hue * 0.22, 1.0],
                    material: [0.15, 0.34, 0.0, 0.06],
                    model_id: MODEL_STANDARD,
                },
            ));
        }

        // The glass shell is transparent, so it goes last and is flagged as the
        // only non-opaque instance.
        let opaque_count = instances.len();
        instances.push((
            "shell".to_string(),
            InstanceData {
                model: Stage::at([0.0, 0.0, 0.0], 1.0),
                color: [0.60, 0.78, 0.92, 0.12],
                material: [0.0, 0.04, 0.92, 0.0],
                model_id: MODEL_GLASS,
            },
        ));

        Some(Stage {
            camera,
            instances,
            opaque_count: Some(opaque_count),
            // No free-standing 3D lines in this scene — the 紐 arrived with
            // `enchu_opener`, which is where `Stage::lines` came from.
            lines: Vec::new(),
        })
    }

    // ---- 2D overlay -----------------------------------------------------

    fn view(&self, ctx: FrameCtx) -> Element {
        // `eased` holds at 1 once the window has passed, so these rise into
        // place and stay up for the rest of the reel.
        let fade = |seq: Seq, dy: f32| -> (f32, f32) {
            let p = seq.eased(ctx, Easing::EaseOutCubic);
            (p, dy * (1.0 - p))
        };
        let (title_a, title_dy) = fade(Seq::new(0.25, 0.7), 28.0);
        let (sub_a, sub_dy) = fade(Seq::new(0.65, 0.7), 20.0);

        div()
            .w_full()
            .h_full()
            .flex_col()
            .justify_end()
            .p_px(96.0)
            .child(
                text("EnchuDB")
                    .font_size(112.0)
                    .color(hex("#f8fafc"))
                    .opacity(title_a)
                    .mb(Px(title_dy + 8.0)),
            )
            .child(
                text("A cylinder you can see into.")
                    .font_size(38.0)
                    .color(hex("#94a3b8"))
                    .opacity(sub_a)
                    .mb(Px(sub_dy)),
            )
    }
}

fn main() {
    let dir = std::env::temp_dir().join("sabitori-reel-stage3d");
    let reel = Reel {
        // Matched to the floor's tone at the horizon so the sky/floor seam
        // doesn't read as a letterbox bar across the top of the frame.
        clear: [0.062, 0.068, 0.082, 1.0],
        ..Reel::new(1920, 1080, FPS, FPS * SECS)
    };
    let paths = reel.render_pngs(&EnchuStage, &dir).expect("render failed");
    println!("{} frames -> {}", paths.len(), dir.display());
}
