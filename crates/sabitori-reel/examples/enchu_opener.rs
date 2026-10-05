//! **円柱** — a 14-second EnchuDB opener, built on the `three-d` stage.
//!
//! Four shots, cut on hard camera changes, over one continuous 3D world:
//!
//!   起  a single empty glass cylinder in the dark, camera low, slow push
//!   承  cells drop in one at a time, then threads link them into a lattice
//!   転  pull back — it was never one cylinder, it is a field of them
//!   結  a query lifts one whole slice out of the lattice, threads and all
//!
//! The data is deliberately *not* pre-formed discs. A row is `CELLS` separate
//! cells on a ring; the ring only becomes a row once the 紐 between them are
//! drawn, and a query is a slice pulled out of that lattice — which is the
//! shape of the thing being described, not a stand-in for it.
//!
//! Every shot is a pure function of the frame: the camera, the springs, the
//! thread opacities and the copy are all derived from `ctx`, nothing
//! accumulates. Frame 300 renders identically whether played or seeked to.
//!
//!   cargo run -p sabitori-reel --example enchu_opener --features three-d --release
//!   cargo run -p sabitori-reel --example enchu_opener --features three-d --release -- --preview
//!
//! Writes `enchu-opener.mp4` beside the PNG sequence in `$TMPDIR`.

use sabitori_reel::three::{
    procedural, Camera, InstanceData, Light, LineVertex, Point3, Renderer, Stage, Vec3D,
    MODEL_GLASS, MODEL_STANDARD,
};
use sabitori_reel::{
    div, interpolate_eased, spring_with, text, Color, Easing, Element, FrameCtx, Px,
    Reel, Scene, Seq, Spring,
};

const FPS: u32 = 30;
const W: u32 = 1920;
const H: u32 = 1080;

/// The four shots, back to back on the reel timeline.
const A: Seq = Seq { start: 0.0, dur: 3.5 }; // 起 — the empty cylinder
const B: Seq = Seq { start: 3.5, dur: 3.5 }; // 承 — cells, then threads
const C: Seq = Seq { start: 7.0, dur: 3.5 }; // 転 — the field
const D: Seq = Seq { start: 10.5, dur: 3.5 }; // 結 — the slice
const TOTAL: f32 = 14.0;

/// The lattice: `ROWS` rings of `CELLS` cells each.
const CELLS: usize = 8;
const ROWS: usize = 6;
const RING_R: f32 = 0.70;
const ROW_H: f32 = 0.42;
const ROW_BASE: f32 = -1.05;
const CELL: f32 = 0.185;
/// The row the query pulls out in 結.
const SLICE_ROW: usize = 3;

/// The field is `SPAN × SPAN` cylinders, hero at the centre.
const SPAN: i32 = 5;
const PITCH: f32 = 4.6;

const SHELL_R: f32 = 1.12;
const SHELL_H: f32 = 2.9;

/// Whether the ground plane is drawn at all.
///
/// A directional light does not fall off, so an infinite floor is lit all the
/// way to the horizon — and the plane *ends* inside the frustum, which is what
/// put a hard horizontal step across frame at y≈124. That step cannot be fixed
/// from the 2D side: a gradient laid over a discontinuity is still a
/// discontinuity, it just lowers its contrast. Without fog in the 3D pass the
/// choice is to hide the join by raising the sky to meet the floor — which is
/// what this scene used to do, and why no pixel in frame was darker than
/// 40/255 — or to drop the floor and let the cylinder stand in the dark, which
/// is what 起 says it does anyway.
const SHOW_FLOOR: bool = false;

fn hex(s: &str) -> Color {
    Color::from_hex(s)
}

/// Teal at the bottom of the stack → violet at the top, so depth inside the
/// cylinder reads as a gradient even where the glass flattens the shading.
fn row_color(row: usize) -> [f32; 3] {
    let k = row as f32 / (ROWS - 1) as f32;
    [0.08 + k * 0.44, 0.74 - k * 0.30, 0.68 + k * 0.20]
}

/// Resting position of one cell, before any slice offset.
fn cell_home(row: usize, i: usize) -> [f32; 3] {
    let theta = i as f32 / CELLS as f32 * std::f32::consts::TAU;
    [
        theta.cos() * RING_R,
        ROW_BASE + row as f32 * ROW_H,
        theta.sin() * RING_R,
    ]
}

/// When cell `(row, i)` arrives. Cells land one at a time, filling the bottom
/// ring first — 単発 arrivals, not a row appearing whole.
fn cell_beat(row: usize, i: usize) -> Seq {
    Seq::new(B.start + 0.12 + (row * CELLS + i) as f32 * 0.05, 0.75)
}

struct Opener;

impl Opener {
    /// How far the queried slice has been pulled out, 0..1. Springs so it
    /// leaves with some weight instead of sliding on rails.
    fn slice_pull(&self, ctx: FrameCtx) -> f32 {
        match Seq::new(D.start + 0.55, 1.2).since(ctx) {
            None => 0.0,
            Some(local) => spring_with(local.t(), FPS, Spring::default()).clamp(0.0, 1.15),
        }
    }

    /// Current position of a hero cell, and how far in its arrival is (0 before
    /// it drops, 1 once landed). `None` if it has not started yet.
    fn cell_at(&self, ctx: FrameCtx, row: usize, i: usize) -> Option<([f32; 3], f32)> {
        let beat = cell_beat(row, i);
        let local = beat.since(ctx)?;
        let drop = spring_with(local.t(), FPS, Spring::default());
        let home = cell_home(row, i);

        // Falls in from above, and the ring it lands on is the one the query
        // later lifts clear of the stack.
        let pull = if row == SLICE_ROW {
            self.slice_pull(ctx)
        } else {
            0.0
        };
        // Pulls out along +x, which at 結's yaw reads as *screen left* and
        // toward camera, so the ring stays large and face-on. It also rises as
        // it leaves, which is what keeps it clear of the lockup below.
        let pos = [
            home[0] + pull * 2.7,
            home[1] + (1.0 - drop) * 3.4 + pull * 0.62,
            home[2],
        ];
        Some((pos, beat.progress(ctx)))
    }
}

impl Scene for Opener {
    // ---- 3D -------------------------------------------------------------

    fn setup_stage(&self, r: &mut Renderer) {
        r.add_mesh("shell", &procedural::cylinder(SHELL_R, SHELL_H, 128), None);
        r.add_mesh("cell", &procedural::cube(CELL), None);
        // Unit cylinder: `Stage::between` stretches it onto each 紐.
        r.add_mesh("thread", &procedural::cylinder(1.0, 1.0, 10), None);
        // Far edge well outside the frustum, so no horizon line reads as an
        // edge — the floor just falls off into the backdrop tone.
        r.add_mesh("floor", &procedural::plane(4000.0, 4000.0, 1, 1), None);

        r.update_lights(
            [0.035, 0.042, 0.06],
            &[
                Light::directional([-0.5, -0.75, -0.45], [0.72, 0.86, 1.0], 1.6),
                // Both rims sit behind the subject so they graze the glass
                // instead of pooling on the floor under the copy.
                Light::point([3.2, 1.4, -3.2], [1.0, 0.62, 0.38], 2.2),
                Light::point([-3.0, 1.1, -2.6], [0.30, 0.92, 0.82], 2.0),
            ],
            false,
        );
    }

    fn stage(&self, ctx: FrameCtx) -> Option<Stage> {
        let t = ctx.t();
        let camera = camera_at(t, (W, H));
        let pull = self.slice_pull(ctx);
        // Once a slice is out, everything left behind steps back.
        let rest_dim = 1.0 - 0.42 * pull.min(1.0);

        // --- opaque ---
        let mut instances: Vec<(String, InstanceData)> = Vec::new();
        if SHOW_FLOOR {
            instances.push((
            "floor".to_string(),
            InstanceData {
                model: Stage::at([0.0, -1.62, 0.0], 1.0),
                // Near-black and rough: a smooth floor catches the key light as
                // one broad specular sheet and turns the lower frame grey.
                //
                // The albedo is a *quarter* of what reads as "dark" by eye. A
                // directional light does not fall off, so this plane is lit
                // edge to edge and fills the lower two thirds of frame; at the
                // previous 0.012 it measured 75–92/255 and there was no black
                // anywhere in the image. Deep enough that the point lights
                // still pool visibly near the subject, and nothing else.
                color: [0.003, 0.0035, 0.0048, 1.0],
                material: [0.0, 0.92, 0.0, 0.0],
                model_id: MODEL_STANDARD,
            },
            ));
        }

        let lines: Vec<LineVertex> = Vec::new();

        // Hero cells, and the 紐 that turn a ring of them into a row.
        for row in 0..ROWS {
            let sliced = row == SLICE_ROW;
            let [r, g, b] = row_color(row);
            // The extracted row reads as "selected" by keeping its own hue and
            // gaining a little emissive — push the emissive much past this and
            // every cell clips to white and the row loses its identity.
            let (tint, glow) = if sliced && pull > 0.01 {
                (1.0 + 0.25 * pull.min(1.0), 0.26 * pull.min(1.0))
            } else {
                (rest_dim, 0.0)
            };

            for i in 0..CELLS {
                let Some((pos, landed)) = self.cell_at(ctx, row, i) else {
                    continue;
                };
                instances.push((
                    "cell".to_string(),
                    InstanceData {
                        model: Stage::spun(pos, 1.0, (row * CELLS + i) as f32 * 0.7),
                        color: [r * tint, g * tint, b * tint, 1.0],
                        material: [0.18, 0.32, 0.0, 0.04 + glow],
                        model_id: MODEL_STANDARD,
                    },
                ));

                // Around the ring: link to the next cell once both have landed.
                // The thread grows out of the join rather than popping in, so
                // `a` scales its radius as well as its brightness.
                let next = (i + 1) % CELLS;
                if let Some((npos, nlanded)) = self.cell_at(ctx, row, next) {
                    let a = ((landed.min(nlanded) - 0.55).max(0.0) / 0.45).min(1.0);
                    if a > 0.03 {
                        instances.push((
                            "thread".to_string(),
                            InstanceData {
                                model: Stage::between(pos, npos, 0.022 * a),
                                color: [r * tint, g * tint, b * tint, 1.0],
                                material: [0.1, 0.3, 0.0, 0.34 * a + glow],
                                model_id: MODEL_STANDARD,
                            },
                        ));
                    }
                }

                // Down the column: link to the same index one row below, so the
                // rows read as one connected structure rather than a stack of
                // loose hoops. The sliced row drops the threads tying it to its
                // neighbours — that is what "extracted" means here.
                if row > 0 && !sliced && row - 1 != SLICE_ROW {
                    if let Some((bpos, blanded)) = self.cell_at(ctx, row - 1, i) {
                        let a = ((landed.min(blanded) - 0.55).max(0.0) / 0.45).min(1.0);
                        if a > 0.03 {
                            let w = 0.7 * rest_dim;
                            instances.push((
                                "thread".to_string(),
                                InstanceData {
                                    model: Stage::between(pos, bpos, 0.013 * a),
                                    color: [r * w, g * w, b * w, 1.0],
                                    material: [0.1, 0.34, 0.0, 0.16 * a],
                                    model_id: MODEL_STANDARD,
                                },
                            ));
                        }
                    }
                }
            }
        }

        let field_shells = self.field(ctx, &mut instances);
        let opaque_count = instances.len();

        // --- transparent, sorted back-to-front from the camera ---
        let cam = camera.position;
        let mut shells: Vec<[f32; 3]> = field_shells;
        shells.push([0.0, 0.0, 0.0]);
        shells.sort_by(|a, b| {
            let d = |p: &[f32; 3]| {
                let (dx, dy, dz) = (
                    p[0] as f64 - cam.x,
                    p[1] as f64 - cam.y,
                    p[2] as f64 - cam.z,
                );
                dx * dx + dy * dy + dz * dz
            };
            d(b).partial_cmp(&d(a)).unwrap_or(std::cmp::Ordering::Equal)
        });
        for pos in shells {
            instances.push((
                "shell".to_string(),
                InstanceData {
                    model: Stage::at(pos, 1.0),
                    color: [0.60, 0.78, 0.92, 0.11],
                    material: [0.0, 0.04, 0.92, 0.0],
                    model_id: MODEL_GLASS,
                },
            ));
        }

        Some(Stage {
            camera,
            instances,
            opaque_count: Some(opaque_count),
            lines,
        })
    }

    // ---- 2D overlay -----------------------------------------------------

    fn view(&self, ctx: FrameCtx) -> Element {
        let t = ctx.t();

        // A caption that fades in, holds for the shot, and fades back out.
        let band = |start: f32, dur: f32| -> f32 {
            let a = interpolate_eased(t, [start, start + 0.45], [0.0, 1.0], Easing::EaseOutCubic);
            let b = interpolate_eased(
                t,
                [start + dur - 0.4, start + dur],
                [0.0, 1.0],
                Easing::EaseIn,
            );
            (a - b).clamp(0.0, 1.0)
        };

        let hero = band(A.start + 0.4, 3.0);
        let cap_b = band(B.start + 0.9, 2.3);
        let cap_c = band(C.start + 1.1, 2.2);
        let lock = band(D.start + 1.6, TOTAL - D.start - 1.6 + 0.4);

        // An `opacity(0)` element still takes up layout space, so a faded-out
        // caption would push everything above it up the frame. Emit only what
        // is actually on screen this frame.
        let mut lines: Vec<Element> = Vec::new();
        if hero > 0.004 {
            lines.push(
                text("円柱")
                    .font_size(148.0)
                    .color(hex("#f1f5f9"))
                    .opacity(hero)
                    .mb(Px(26.0 * (1.0 - hero))),
            );
        }
        if cap_b > 0.004 {
            lines.push(
                text("one row at a time — then linked")
                    .font_size(40.0)
                    .color(hex("#cbd5e1"))
                    .opacity(cap_b),
            );
        }
        if cap_c > 0.004 {
            lines.push(
                text("one file — a field of cylinders")
                    .font_size(40.0)
                    .color(hex("#cbd5e1"))
                    .opacity(cap_c),
            );
        }
        if lock > 0.004 {
            lines.push(
                text("EnchuDB")
                    .font_size(96.0)
                    .color(hex("#f1f5f9"))
                    .opacity(lock)
                    .mb(Px(8.0)),
            );
            lines.push(
                text("a query is a slice")
                    .font_size(36.0)
                    .color(hex("#5eead4"))
                    .opacity(lock),
            );
        }

        // Copy sits over glass that is often near-white, so it needs a scrim:
        // a gradient block pinned to the bottom of a flex column, transparent
        // at its top edge and dark by the baseline. The matching block at the
        // top is now only there for mood — the horizon seam is fixed where it
        // belongs, by matching `clear` to the floor rather than painting over
        // the join.
        let scrim_bottom = div()
            .w_full()
            .flex_col()
            .gradient(
                Color::new(0.0, 0.0, 0.0, 0.0),
                Color::new(0.015, 0.022, 0.038, 0.95),
                std::f32::consts::FRAC_PI_2,
            )
            .pt(Px(230.0))
            .pb(Px(104.0))
            .pl(Px(104.0))
            .pr(Px(104.0))
            .children(lines);

        // Taller and much lighter than it was: at 0.95 over a sky that is
        // already near-black it only crushed the top of frame flat.
        let sky = div().w_full().h(Px(520.0)).gradient(
            Color::new(0.010, 0.013, 0.020, 0.55),
            Color::new(0.0, 0.0, 0.0, 0.0),
            std::f32::consts::FRAC_PI_2,
        );

        div()
            .w_full()
            .h_full()
            .flex_col()
            .justify_between()
            .child(sky)
            .child(scrim_bottom)
    }
}

impl Opener {
    /// The surrounding field, which only exists during 転. Cylinders pop in
    /// staggered by their distance from the hero, so the reveal spreads outward
    /// instead of appearing all at once. Neighbours get cells but no threads —
    /// at that distance the 紐 would only read as noise.
    fn field(&self, ctx: FrameCtx, out: &mut Vec<(String, InstanceData)>) -> Vec<[f32; 3]> {
        let mut shells = Vec::new();
        // 結 collapses the field back down so the last shot is the hero alone.
        let exit = interpolate_eased(
            ctx.t(),
            [D.start + 0.05, D.start + 0.95],
            [1.0, 0.0],
            Easing::EaseIn,
        );
        if exit < 0.02 {
            return shells;
        }
        let half = SPAN / 2;
        for gx in -half..=half {
            for gz in -half..=half {
                if gx == 0 && gz == 0 {
                    continue; // the hero is placed separately
                }
                let ring = gx.abs().max(gz.abs()) as f32;
                let beat = Seq::new(C.start + 0.25 + ring * 0.34, 0.9);
                let Some(local) = beat.since(ctx) else {
                    continue;
                };
                let pop = spring_with(local.t(), FPS, Spring::default()).min(1.35) * exit;
                let base = [gx as f32 * PITCH, 0.0, gz as f32 * PITCH];

                // Deterministic per-cell variation — no RNG, so it is stable
                // under scrubbing.
                let seed = (gx * 7 + gz * 13) as f32;
                let fill = 2 + ((seed.abs() as usize * 3) % (ROWS - 1));
                for row in 0..fill {
                    let [r, g, b] = row_color(row);
                    for i in 0..CELLS {
                        let home = cell_home(row, i);
                        out.push((
                            "cell".to_string(),
                            InstanceData {
                                model: Stage::spun(
                                    [base[0] + home[0], home[1] * pop, base[2] + home[2]],
                                    pop,
                                    seed * 0.21 + i as f32 * 0.7,
                                ),
                                color: [r * 0.8, g * 0.8, b * 0.8, 1.0],
                                material: [0.18, 0.36, 0.0, 0.03],
                                model_id: MODEL_STANDARD,
                            },
                        ));
                    }
                }
                shells.push(base);
            }
        }
        shells
    }
}

/// Camera for time `t` — a hard cut at each shot boundary, smooth within.
fn camera_at(t: f32, aspect: (u32, u32)) -> Camera {
    // (yaw, distance, height, target height, fov) at the start and end of the
    // shot we are inside.
    let (from, to, p): ([f32; 5], [f32; 5], f32) = if t < B.start {
        let p = (t / A.dur).clamp(0.0, 1.0);
        // Distances are set from the frustum, not by eye: at fov f the camera
        // needs `half_extent / tan(f/2)` to fit a 3-unit cylinder with
        // headroom. Closer than that and the subject crops to a blank wall.
        (
            [-0.42, 9.4, 0.85, 0.15, 33.0],
            [-0.18, 7.9, 1.35, 0.15, 33.0],
            Easing::EaseInOut.eval(p),
        )
    } else if t < C.start {
        let p = ((t - B.start) / B.dur).clamp(0.0, 1.0);
        (
            [0.62, 8.4, 2.90, 0.15, 38.0],
            [1.18, 7.0, 1.90, 0.15, 38.0],
            Easing::EaseOutCubic.eval(p),
        )
    } else if t < D.start {
        let p = ((t - C.start) / C.dur).clamp(0.0, 1.0);
        (
            [1.52, 8.6, 2.60, 0.10, 42.0],
            [1.98, 27.0, 10.5, 0.00, 42.0],
            Easing::EaseInOut.eval(p),
        )
    } else {
        let p = ((t - D.start) / D.dur).clamp(0.0, 1.0);
        // Back in close on the hero. The field cannot be shot from inside —
        // the hero sits at its centre, so a camera near enough to frame it is
        // always behind a neighbour. So the field clears out instead (see
        // `field`), and 結 returns to the single cylinder it opened on. The
        // yaw is chosen so the slice pulls out across frame, not toward camera.
        (
            [2.05, 8.8, 2.30, 0.15, 34.0],
            [2.24, 7.8, 1.80, 0.15, 34.0],
            Easing::EaseOutCubic.eval(p),
        )
    };
    let lerp = |i: usize| from[i] + (to[i] - from[i]) * p;
    let (yaw, dist, y, ty, fov) = (lerp(0), lerp(1), lerp(2), lerp(3), lerp(4));

    let (s, c) = yaw.sin_cos();
    let mut cam = Camera::new();
    cam.position = Point3::new((s * dist) as f64, y as f64, (c * dist) as f64);
    cam.target = Point3::new(0.0, ty as f64, 0.0);
    // seimei's defaults are CAD-scale (up = Z, near = 10); this world is metres.
    cam.up = Vec3D::new(0.0, 1.0, 0.0);
    cam.fov = fov as f64;
    cam.near = 0.05;
    cam.far = 9000.0;
    cam.set_aspect(aspect.0, aspect.1);
    cam
}

fn main() {
    let dir = std::env::temp_dir().join("enchu-opener");
    let reel = Reel {
        // Matched to the floor tone *at the horizon* so the seam does not read
        // as a letterbox bar — the floor plane ends inside the frustum and the
        // 3D pass can only clear to one flat colour, so the two have to meet at
        // the same value or a hard line runs across frame.
        //
        // These are linear values written into an sRGB target: 0.0074 linear is
        // ~21/255 on screen, which is where the darkened floor now lands just
        // below the horizon. The old 0.062 was ~70/255 — brighter than the
        // floor it was supposed to match, and the top scrim was doing the work
        // of hiding the difference.
        clear: [0.0074, 0.0081, 0.0098, 1.0],
        ..Reel::new(W, H, FPS, (TOTAL * FPS as f32) as u32)
    };

    // `-- --preview` opens the scrubbable window instead of exporting. Same
    // scene, same renderers, same frame numbers — the window just presents
    // what the export would have written.
    if std::env::args().any(|a| a == "--preview") {
        reel.preview(Opener);
        return;
    }

    let mp4 = dir.join("enchu-opener.mp4");
    match reel.render_mp4(&Opener, &mp4) {
        Ok(path) => println!("mp4 -> {}", path.display()),
        Err(e) => {
            eprintln!("mp4 failed ({e}); falling back to a PNG sequence");
            let paths = reel.render_pngs(&Opener, &dir).expect("render failed");
            println!("{} frames -> {}", paths.len(), dir.display());
        }
    }
}
