#![allow(
    unused_variables,
    unused_mut,
    unused_parens,
    dead_code,
    clippy::needless_range_loop,
    clippy::too_many_arguments
)]

//! Sabitori GPU Showcase — 30 demos in a grid with modal expansion.
//! Run: cargo build --release --example showcase

use std::sync::Arc;
use std::time::Instant;

use sabitori::{Color, RectInstance, TextRenderer};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::keyboard::Key;
use winit::window::{Window, WindowAttributes, WindowId};

// ---------------------------------------------------------------------------
// Constants
// ---------------------------------------------------------------------------
const DEMO_COUNT: usize = 30;
const GRID_COLS: usize = 6;
const CARD_PAD: f32 = 12.0;
const CARD_RADIUS: f32 = 10.0;
const HEADER_H: f32 = 50.0;
const MODAL_ANIM_SPEED: f32 = 6.0;
const SCROLL_SPEED: f32 = 40.0;

// Tokyo Night palette
const BG: &str = "#1a1b26";
const BG2: &str = "#24283b";
const BG3: &str = "#343a52";
const TEXT_COL: &str = "#c0caf5";
const TEXT2: &str = "#9aa5ce";
const PRIMARY: &str = "#7aa2f7";
const BORDER: &str = "#414868";
const SUCCESS: &str = "#9ece6a";
const WARNING: &str = "#e0af68";
const ERROR: &str = "#f7768e";
const PURPLE: &str = "#bb9af7";
const CYAN: &str = "#7dcfff";
const TEAL: &str = "#73daca";
const ORANGE: &str = "#ff9e64";

fn c(hex: &str) -> Color {
    Color::from_hex(hex)
}

// ---------------------------------------------------------------------------
// Helper rect constructors
// ---------------------------------------------------------------------------
fn flat_rect(x: f32, y: f32, w: f32, h: f32, r: f32, fill: Color) -> RectInstance {
    RectInstance {
        rect: [x, y, w, h],
        fill_color: fill.to_array(),
        corner_radii: [r; 4],
        border_color: [0.0; 4],
        border_width: 0.0,
        gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
        shadow_color: [0.0; 4],
        shadow_offset: [0.0; 2],
        shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
    }
}

fn bordered_rect(
    x: f32, y: f32, w: f32, h: f32, r: f32,
    fill: Color, border: Color, bw: f32,
) -> RectInstance {
    RectInstance {
        rect: [x, y, w, h],
        fill_color: fill.to_array(),
        corner_radii: [r; 4],
        border_color: border.to_array(),
        border_width: bw,
        gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
        shadow_color: [0.0; 4],
        shadow_offset: [0.0; 2],
        shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
    }
}

fn shadow_rect(
    x: f32, y: f32, w: f32, h: f32, r: f32,
    fill: Color, sc: Color, so: [f32; 2], sp: [f32; 2],
) -> RectInstance {
    RectInstance {
        rect: [x, y, w, h],
        fill_color: fill.to_array(),
        corner_radii: [r; 4],
        border_color: [0.0; 4],
        border_width: 0.0,
        gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
        shadow_color: sc.to_array(),
        shadow_offset: so,
        shadow_params: sp,
        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
    }
}

fn circle_rect(cx: f32, cy: f32, r: f32, fill: Color) -> RectInstance {
    flat_rect(cx - r, cy - r, r * 2.0, r * 2.0, r, fill)
}

// ---------------------------------------------------------------------------
// HSV → Color
// ---------------------------------------------------------------------------
fn hue_to_color(hue: f32, sat: f32, val: f32, alpha: f32) -> Color {
    let h = ((hue % 360.0) + 360.0) % 360.0;
    let s = sat.clamp(0.0, 1.0);
    let v = val.clamp(0.0, 1.0);
    let cc = v * s;
    let x = cc * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - cc;
    let (r, g, b) = if h < 60.0 { (cc, x, 0.0) }
        else if h < 120.0 { (x, cc, 0.0) }
        else if h < 180.0 { (0.0, cc, x) }
        else if h < 240.0 { (0.0, x, cc) }
        else if h < 300.0 { (x, 0.0, cc) }
        else { (cc, 0.0, x) };
    Color::new(r + m, g + m, b + m, alpha)
}

fn speed_color(hue: f32, speed: f32, alpha: f32) -> Color {
    let f = (speed / 500.0).min(1.0);
    let h = (hue + f * 60.0) % 360.0;
    hue_to_color(h, 0.7 + f * 0.3, 0.6 + f * 0.4, alpha)
}

// ---------------------------------------------------------------------------
// Particle
// ---------------------------------------------------------------------------
#[derive(Clone)]
struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,
    size: f32,
    hue: f32,
}

impl Particle {
    fn new(x: f32, y: f32, hue: f32) -> Self {
        Self { x, y, vx: 0.0, vy: 0.0, life: 1.0, size: 3.0, hue }
    }

    fn new_full(x: f32, y: f32, vx: f32, vy: f32, life: f32, size: f32, hue: f32) -> Self {
        Self { x, y, vx, vy, life, size, hue }
    }
}

// ---------------------------------------------------------------------------
// Per-demo state
// ---------------------------------------------------------------------------
struct DemoState {
    particles: Vec<Particle>,
    time: f32,
    click_count: u32,
    values: [f32; 32],
    active: bool,
    initialized: bool,
}

impl DemoState {
    fn new() -> Self {
        Self {
            particles: Vec::new(),
            time: 0.0,
            click_count: 0,
            values: [0.0; 32],
            active: false,
            initialized: false,
        }
    }
}

// ---------------------------------------------------------------------------
// Demo descriptor
// ---------------------------------------------------------------------------
struct DemoInfo {
    name: &'static str,
    desc: &'static str,
    gpu_only: bool,
    category: &'static str,
}

fn demo_infos() -> Vec<DemoInfo> {
    vec![
        // GPU Powerhouse (0-9)
        DemoInfo { name: "Particle Gravity",  desc: "5000 particles with gravity + mouse repulsion", gpu_only: true, category: "GPU Powerhouse" },
        DemoInfo { name: "Particle Orbit",    desc: "5000 particles orbiting mouse position",        gpu_only: true, category: "GPU Powerhouse" },
        DemoInfo { name: "Particle Explosion", desc: "5000 particles explode from click point",      gpu_only: true, category: "GPU Powerhouse" },
        DemoInfo { name: "Wave Field",         desc: "5000 particles forming sine wave, mouse disturbs", gpu_only: true, category: "GPU Powerhouse" },
        DemoInfo { name: "Particle Swarm",     desc: "5000 particles flocking toward mouse then scattering", gpu_only: true, category: "GPU Powerhouse" },
        DemoInfo { name: "N-Body Gravity",     desc: "200 larger particles with mutual gravitational attraction", gpu_only: true, category: "GPU Powerhouse" },
        DemoInfo { name: "Fluid Grid",         desc: "50x50 grid of dots that ripple from mouse",   gpu_only: true, category: "GPU Powerhouse" },
        DemoInfo { name: "Cloth Sim",          desc: "Grid of connected particles (verlet integration)", gpu_only: true, category: "GPU Powerhouse" },
        DemoInfo { name: "Fireworks",          desc: "Click to launch, explode into 500 particles with trails", gpu_only: true, category: "GPU Powerhouse" },
        DemoInfo { name: "Galaxy",             desc: "3000 particles in spiral galaxy rotation",     gpu_only: true, category: "GPU Powerhouse" },
        // Interactive UI (10-14)
        DemoInfo { name: "Spring Balls",    desc: "5 balls connected by springs, drag any ball",  gpu_only: false, category: "Interactive UI" },
        DemoInfo { name: "macOS Dock",      desc: "Magnifying icons on hover (proximity-based)",  gpu_only: false, category: "Interactive UI" },
        DemoInfo { name: "Parallax Layers", desc: "5 layers following mouse at different speeds", gpu_only: false, category: "Interactive UI" },
        DemoInfo { name: "Card Tilt 3D",    desc: "Card shadow/highlight shifts with mouse (3D)", gpu_only: false, category: "Interactive UI" },
        DemoInfo { name: "Elastic Cursor",  desc: "Blob that follows mouse with spring lag",      gpu_only: false, category: "Interactive UI" },
        // Animated UI (15-19)
        DemoInfo { name: "Toggle Switch",  desc: "Animated pill switch with smooth transition",    gpu_only: false, category: "Animated UI" },
        DemoInfo { name: "Morph Button",   desc: "Submit -> spinner -> Done",                      gpu_only: false, category: "Animated UI" },
        DemoInfo { name: "Loading Wave",   desc: "12-bar audio equalizer animation",               gpu_only: false, category: "Animated UI" },
        DemoInfo { name: "Stagger Reveal", desc: "Items appearing sequentially with stagger delay", gpu_only: false, category: "Animated UI" },
        DemoInfo { name: "Ripple Click",   desc: "Material design ripple effect on click",         gpu_only: false, category: "Animated UI" },
        // Data Viz (20-24)
        DemoInfo { name: "Live Bar Chart", desc: "10 bars with random animated values",            gpu_only: false, category: "Data Viz" },
        DemoInfo { name: "Sparkline",      desc: "Scrolling line chart with new data points",      gpu_only: false, category: "Data Viz" },
        DemoInfo { name: "Donut Chart",    desc: "Animated circular chart segments",               gpu_only: false, category: "Data Viz" },
        DemoInfo { name: "Heatmap",        desc: "20x20 color grid with animated values",          gpu_only: false, category: "Data Viz" },
        DemoInfo { name: "Network Graph",  desc: "30 nodes with connecting lines, force-directed", gpu_only: false, category: "Data Viz" },
        // Effects (25-29)
        DemoInfo { name: "Matrix Rain",  desc: "Falling green characters (200+ columns)", gpu_only: true, category: "Effects" },
        DemoInfo { name: "Starfield",    desc: "1000 stars zooming past (parallax depth)", gpu_only: true, category: "Effects" },
        DemoInfo { name: "Aurora",       desc: "Undulating color bands",                   gpu_only: true, category: "Effects" },
        DemoInfo { name: "Rain",         desc: "500 falling drops with splash",            gpu_only: true, category: "Effects" },
        DemoInfo { name: "Fireflies",    desc: "200 glowing dots drifting randomly",       gpu_only: true, category: "Effects" },
    ]
}

// ---------------------------------------------------------------------------
// Pseudo-random (deterministic, no deps)
// ---------------------------------------------------------------------------
struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Self { Self(seed) }
    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        self.0
    }
    fn f32(&mut self) -> f32 { (self.next() >> 33) as f32 / (1u64 << 31) as f32 }
    fn range(&mut self, lo: f32, hi: f32) -> f32 { lo + self.f32() * (hi - lo) }
}

// ---------------------------------------------------------------------------
// App state
// ---------------------------------------------------------------------------
struct ShowcaseApp {
    window: Option<Arc<Window>>,
    renderer: Option<sabitori::GpuRenderer>,
    text_renderer: Option<TextRenderer>,
    last_frame: Instant,
    win_w: f32,
    win_h: f32,
    mouse_x: f32,
    mouse_y: f32,
    mouse_pressed: bool,
    scroll_y: f32,
    frame_count: u32,
    fps_timer: Instant,
    fps: f32,

    modal_open: Option<usize>,
    modal_anim: f32, // 0=closed, 1=open
    modal_target: f32,

    demos: Vec<DemoState>,
    infos: Vec<DemoInfo>,
    rng: Rng,
    global_time: f32,

    // For demos that need click position in modal
    modal_click_x: f32,
    modal_click_y: f32,
}

impl ShowcaseApp {
    fn new() -> Self {
        let mut demos = Vec::with_capacity(DEMO_COUNT);
        for _ in 0..DEMO_COUNT {
            demos.push(DemoState::new());
        }
        Self {
            window: None,
            renderer: None,
            text_renderer: None,
            last_frame: Instant::now(),
            win_w: 1400.0,
            win_h: 900.0,
            mouse_x: 700.0,
            mouse_y: 450.0,
            mouse_pressed: false,
            scroll_y: 0.0,
            frame_count: 0,
            fps_timer: Instant::now(),
            fps: 0.0,
            modal_open: None,
            modal_anim: 0.0,
            modal_target: 0.0,
            demos,
            infos: demo_infos(),
            rng: Rng::new(42),
            global_time: 0.0,
            modal_click_x: 0.0,
            modal_click_y: 0.0,
        }
    }

    // ------ Grid layout helpers ------

    fn card_size(&self) -> (f32, f32) {
        let usable = self.win_w - CARD_PAD * (GRID_COLS as f32 + 1.0);
        let cw = usable / GRID_COLS as f32;
        let ch = cw * 0.75;
        (cw, ch)
    }

    fn card_rect(&self, idx: usize) -> (f32, f32, f32, f32) {
        let (cw, ch) = self.card_size();
        let col = idx % GRID_COLS;
        let row = idx / GRID_COLS;
        let x = CARD_PAD + col as f32 * (cw + CARD_PAD);
        let y = HEADER_H + CARD_PAD + row as f32 * (ch + CARD_PAD + 28.0) - self.scroll_y;
        (x, y, cw, ch)
    }

    fn modal_rect(&self) -> (f32, f32, f32, f32) {
        let mw = self.win_w * 0.82;
        let mh = self.win_h * 0.82;
        let mx = (self.win_w - mw) * 0.5;
        let my = (self.win_h - mh) * 0.5;
        (mx, my, mw, mh)
    }

    fn demo_area_in_modal(&self) -> (f32, f32, f32, f32) {
        let (mx, my, mw, mh) = self.modal_rect();
        (mx + 16.0, my + 50.0, mw - 32.0, mh - 66.0)
    }

    /// Get the render area for a demo. Returns (x, y, w, h).
    /// If it's a card preview, returns the card interior. If modal, returns the modal interior.
    fn demo_render_area(&self, idx: usize, is_modal: bool) -> (f32, f32, f32, f32) {
        if is_modal {
            self.demo_area_in_modal()
        } else {
            let (cx, cy, cw, ch) = self.card_rect(idx);
            (cx + 4.0, cy + 4.0, cw - 8.0, ch - 8.0)
        }
    }

    /// Particle count to use depending on card vs modal
    fn particle_count(&self, idx: usize, is_modal: bool) -> usize {
        if is_modal {
            match idx {
                0..=4 => 5000,
                5 => 200,
                6 => 2500, // 50x50
                7 => 900,  // 30x30 cloth
                8 => 2000,
                9 => 3000,
                25 => 4000, // matrix
                26 => 1000, // starfield
                27 => 2000, // aurora
                28 => 500,  // rain
                29 => 200,  // fireflies
                _ => 100,
            }
        } else {
            match idx {
                0..=4 => 300,
                5 => 30,
                6 => 400, // 20x20
                7 => 100, // 10x10 cloth
                8 => 200,
                9 => 200,
                25 => 300,
                26 => 150,
                27 => 200,
                28 => 60,
                29 => 30,
                _ => 20,
            }
        }
    }

    // ------ Initialize demo particles ------
    fn init_demo(&mut self, idx: usize, is_modal: bool) {
        let (ax, ay, aw, ah) = self.demo_render_area(idx, is_modal);
        let count = self.particle_count(idx, is_modal);
        let state = &mut self.demos[idx];
        state.particles.clear();
        state.time = 0.0;
        state.click_count = 0;
        state.values = [0.0; 32];
        state.initialized = true;

        match idx {
            // Particle Gravity
            0 => {
                for i in 0..count {
                    let frac = i as f32 / count as f32;
                    let px = ax + self.rng.range(0.0, aw);
                    let py = ay + self.rng.range(0.0, ah * 0.3);
                    state.particles.push(Particle::new_full(px, py, self.rng.range(-30.0, 30.0), 0.0, 1.0, 3.0, frac * 360.0));
                }
            }
            // Particle Orbit
            1 => {
                let cx = ax + aw * 0.5;
                let cy = ay + ah * 0.5;
                for i in 0..count {
                    let frac = i as f32 / count as f32;
                    let angle = frac * std::f32::consts::TAU * 5.0;
                    let r = 20.0 + frac * (aw.min(ah) * 0.4);
                    let px = cx + angle.cos() * r;
                    let py = cy + angle.sin() * r;
                    state.particles.push(Particle::new_full(px, py, 0.0, 0.0, 1.0, 2.5, frac * 360.0));
                }
            }
            // Particle Explosion
            2 => {
                let cx = ax + aw * 0.5;
                let cy = ay + ah * 0.5;
                for i in 0..count {
                    let frac = i as f32 / count as f32;
                    let angle = frac * std::f32::consts::TAU;
                    let speed = self.rng.range(100.0, 600.0);
                    state.particles.push(Particle::new_full(cx, cy, angle.cos() * speed, angle.sin() * speed, 1.0, self.rng.range(2.0, 5.0), frac * 360.0));
                }
            }
            // Wave Field
            3 => {
                for i in 0..count {
                    let frac = i as f32 / count as f32;
                    let px = ax + frac * aw;
                    let py = ay + ah * 0.5;
                    state.particles.push(Particle::new_full(px, py, 0.0, 0.0, 1.0, 3.0, frac * 360.0));
                }
            }
            // Particle Swarm
            4 => {
                for i in 0..count {
                    let frac = i as f32 / count as f32;
                    let px = ax + self.rng.range(0.0, aw);
                    let py = ay + self.rng.range(0.0, ah);
                    state.particles.push(Particle::new_full(px, py, self.rng.range(-50.0, 50.0), self.rng.range(-50.0, 50.0), 1.0, 3.0, frac * 360.0));
                }
            }
            // N-Body Gravity
            5 => {
                let cx = ax + aw * 0.5;
                let cy = ay + ah * 0.5;
                for i in 0..count {
                    let frac = i as f32 / count as f32;
                    let angle = frac * std::f32::consts::TAU;
                    let r = self.rng.range(30.0, aw.min(ah) * 0.35);
                    let px = cx + angle.cos() * r;
                    let py = cy + angle.sin() * r;
                    let tang_speed = (100.0 / r.max(1.0)).sqrt() * 60.0;
                    state.particles.push(Particle::new_full(
                        px, py,
                        -angle.sin() * tang_speed, angle.cos() * tang_speed,
                        1.0, self.rng.range(4.0, 10.0), frac * 360.0,
                    ));
                }
            }
            // Fluid Grid
            6 => {
                let side = (count as f32).sqrt() as usize;
                let dx = aw / (side as f32 + 1.0);
                let dy = ah / (side as f32 + 1.0);
                for row in 0..side {
                    for col in 0..side {
                        let px = ax + dx * (col as f32 + 1.0);
                        let py = ay + dy * (row as f32 + 1.0);
                        let hue = (row as f32 / side as f32) * 180.0 + (col as f32 / side as f32) * 180.0;
                        state.particles.push(Particle::new_full(px, py, px, py, 1.0, 3.0, hue));
                        // vx,vy store rest position for this demo
                    }
                }
            }
            // Cloth Sim
            7 => {
                let side = (count as f32).sqrt() as usize;
                let dx = aw * 0.7 / (side as f32);
                let dy = ah * 0.7 / (side as f32);
                let ox = ax + aw * 0.15;
                let oy = ay + ah * 0.1;
                for row in 0..side {
                    for col in 0..side {
                        let px = ox + dx * col as f32;
                        let py = oy + dy * row as f32;
                        let hue = (row as f32 / side as f32) * 240.0;
                        // vx,vy = previous position for verlet
                        state.particles.push(Particle::new_full(px, py, px, py, 1.0, 2.5, hue));
                    }
                }
                state.values[0] = side as f32; // store grid size
                state.values[1] = dx;
                state.values[2] = dy;
            }
            // Fireworks
            8 => {
                // Start with a few rockets
                for i in 0..3 {
                    let frac = i as f32 / 3.0;
                    let px = ax + aw * (0.2 + frac * 0.6);
                    state.particles.push(Particle::new_full(px, ay + ah, 0.0, -self.rng.range(200.0, 400.0), 1.0, 4.0, frac * 120.0));
                }
            }
            // Galaxy
            9 => {
                let cx = ax + aw * 0.5;
                let cy = ay + ah * 0.5;
                for i in 0..count {
                    let frac = i as f32 / count as f32;
                    let arm = (i % 3) as f32;
                    let base_angle = arm * std::f32::consts::TAU / 3.0;
                    let r = 10.0 + frac * (aw.min(ah) * 0.45);
                    let spiral = frac * 4.0;
                    let angle = base_angle + spiral;
                    let spread = self.rng.range(-15.0, 15.0);
                    let px = cx + (angle.cos() * r) + spread;
                    let py = cy + (angle.sin() * r * 0.6) + self.rng.range(-10.0, 10.0);
                    state.particles.push(Particle::new_full(px, py, 0.0, 0.0, 1.0, self.rng.range(1.5, 3.5), frac * 360.0));
                }
            }
            // Spring Balls
            10 => {
                let spacing = aw / 6.0;
                for i in 0..5 {
                    let px = ax + spacing * (i as f32 + 1.0);
                    let py = ay + ah * 0.5;
                    state.particles.push(Particle::new_full(px, py, px, py, 1.0, 18.0, i as f32 * 72.0));
                    // vx,vy = target position
                }
            }
            // macOS Dock
            11 => {
                let icon_count = if is_modal { 12 } else { 8 };
                for i in 0..icon_count {
                    state.particles.push(Particle::new_full(0.0, 0.0, 0.0, 0.0, 1.0, 40.0, i as f32 * 30.0));
                }
            }
            // Parallax Layers
            12 => {
                for layer in 0..5 {
                    let layer_count = if is_modal { 20 } else { 8 };
                    for i in 0..layer_count {
                        let px = self.rng.range(ax, ax + aw);
                        let py = self.rng.range(ay, ay + ah);
                        state.particles.push(Particle::new_full(px, py, px, py, layer as f32 / 4.0, self.rng.range(4.0, 20.0 - layer as f32 * 2.0), layer as f32 * 60.0));
                    }
                }
            }
            // Card Tilt 3D
            13 => {
                // Single card — store rotation in values
            }
            // Elastic Cursor
            14 => {
                let cx = ax + aw * 0.5;
                let cy = ay + ah * 0.5;
                // One particle for the blob
                state.particles.push(Particle::new_full(cx, cy, 0.0, 0.0, 1.0, 20.0, 200.0));
            }
            // Toggle Switch
            15 => {
                state.values[0] = 0.0; // off/on anim
            }
            // Morph Button
            16 => {
                state.values[0] = 0.0; // phase: 0=idle, 1=loading, 2=done
                state.values[1] = 0.0; // anim progress
            }
            // Loading Wave
            17 => {
                // 12 bars
                for i in 0..12 {
                    state.values[i] = self.rng.f32();
                }
            }
            // Stagger Reveal
            18 => {
                // 8 items with stagger
                for i in 0..8 {
                    state.values[i] = 0.0; // alpha/offset
                }
            }
            // Ripple Click
            19 => {
                // ripples stored in particles
            }
            // Live Bar Chart
            20 => {
                for i in 0..10 {
                    state.values[i] = self.rng.range(0.1, 1.0);
                    state.values[10 + i] = state.values[i]; // target
                }
            }
            // Sparkline
            21 => {
                for i in 0..32 {
                    state.values[i] = self.rng.range(0.2, 0.8);
                }
            }
            // Donut Chart
            22 => {
                let mut total = 0.0f32;
                for i in 0..5 {
                    state.values[i] = self.rng.range(0.5, 3.0);
                    total += state.values[i];
                }
                // normalize
                for i in 0..5 { state.values[i] /= total; }
                state.values[5] = 0.0; // anim progress
            }
            // Heatmap
            23 => {
                // nothing special, animated per frame
            }
            // Network Graph
            24 => {
                let node_count = if is_modal { 30 } else { 12 };
                for i in 0..node_count {
                    let px = ax + self.rng.range(aw * 0.1, aw * 0.9);
                    let py = ay + self.rng.range(ah * 0.1, ah * 0.9);
                    state.particles.push(Particle::new_full(px, py, self.rng.range(-10.0, 10.0), self.rng.range(-10.0, 10.0), 1.0, self.rng.range(5.0, 12.0), i as f32 * (360.0 / node_count as f32)));
                }
            }
            // Matrix Rain
            25 => {
                let cols = if is_modal { 200 } else { 30 };
                for i in 0..count.min(cols * 20) {
                    let col = i % cols;
                    let px = ax + (col as f32 / cols as f32) * aw;
                    let py = ay - self.rng.range(0.0, ah * 2.0);
                    state.particles.push(Particle::new_full(px, py, col as f32, self.rng.range(80.0, 250.0), self.rng.f32(), 10.0, 120.0));
                    // vx = column index, vy = fall speed
                }
            }
            // Starfield
            26 => {
                let cx = ax + aw * 0.5;
                let cy = ay + ah * 0.5;
                for i in 0..count {
                    let angle = self.rng.range(0.0, std::f32::consts::TAU);
                    let dist = self.rng.range(0.0, 1.0);
                    state.particles.push(Particle::new_full(
                        cx + angle.cos() * dist * aw * 0.5,
                        cy + angle.sin() * dist * ah * 0.5,
                        angle.cos(), angle.sin(),
                        dist, // depth factor stored as life
                        self.rng.range(1.0, 3.0),
                        200.0 + self.rng.range(0.0, 60.0),
                    ));
                }
            }
            // Aurora
            27 => {
                for i in 0..count {
                    let frac = i as f32 / count as f32;
                    let band = (i % 5) as f32;
                    let px = ax + frac * aw;
                    let py = ay + ah * (0.2 + band * 0.12);
                    state.particles.push(Particle::new_full(px, py, px, band, 1.0, 6.0, 120.0 + band * 40.0));
                }
            }
            // Rain
            28 => {
                for i in 0..count {
                    let px = ax + self.rng.range(0.0, aw);
                    let py = ay + self.rng.range(-ah, ah);
                    state.particles.push(Particle::new_full(px, py, self.rng.range(-5.0, 5.0), self.rng.range(200.0, 500.0), 1.0, self.rng.range(1.0, 3.0), 210.0));
                }
            }
            // Fireflies
            29 => {
                for i in 0..count {
                    let px = ax + self.rng.range(0.0, aw);
                    let py = ay + self.rng.range(0.0, ah);
                    state.particles.push(Particle::new_full(px, py, self.rng.range(-20.0, 20.0), self.rng.range(-20.0, 20.0), self.rng.f32(), self.rng.range(3.0, 8.0), 50.0 + self.rng.range(0.0, 30.0)));
                }
            }
            _ => {}
        }
    }

    // ------ Update a single demo ------
    fn update_demo(&mut self, idx: usize, dt: f32, is_modal: bool) {
        let (ax, ay, aw, ah) = self.demo_render_area(idx, is_modal);
        let mx = self.mouse_x;
        let my = self.mouse_y;
        let pressed = self.mouse_pressed;
        let gt = self.global_time;

        let state = &mut self.demos[idx];
        state.time += dt;
        let t = state.time;

        match idx {
            // Particle Gravity
            0 => {
                for p in &mut state.particles {
                    p.vy += 300.0 * dt;
                    if pressed {
                        let dx = p.x - mx;
                        let dy = p.y - my;
                        let dist_sq = dx * dx + dy * dy + 1.0;
                        let dist = dist_sq.sqrt();
                        if dist < 120.0 {
                            let force = 50000.0 / dist_sq;
                            p.vx += dx / dist * force * dt;
                            p.vy += dy / dist * force * dt;
                        }
                    }
                    p.vx *= 0.98;
                    p.vy *= 0.98;
                    p.x += p.vx * dt;
                    p.y += p.vy * dt;
                    bounce_in_area(p, ax, ay, aw, ah);
                }
            }
            // Particle Orbit
            1 => {
                let target_x = if is_modal { mx } else { ax + aw * 0.5 + (t * 0.8).sin() * aw * 0.2 };
                let target_y = if is_modal { my } else { ay + ah * 0.5 + (t * 0.6).cos() * ah * 0.2 };
                for p in &mut state.particles {
                    let dx = target_x - p.x;
                    let dy = target_y - p.y;
                    let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                    let attract = 150.0;
                    p.vx += dx / dist * attract * dt;
                    p.vy += dy / dist * attract * dt;
                    // Orbital tangent
                    p.vx += -dy / dist * 80.0 * dt;
                    p.vy += dx / dist * 80.0 * dt;
                    p.vx *= 0.98;
                    p.vy *= 0.98;
                    p.x += p.vx * dt;
                    p.y += p.vy * dt;
                    clamp_in_area(p, ax, ay, aw, ah);
                }
            }
            // Particle Explosion
            2 => {
                for p in &mut state.particles {
                    p.vy += 100.0 * dt;
                    p.vx += (t * 0.5).sin() * 20.0 * dt;
                    p.vx *= 0.97;
                    p.vy *= 0.97;
                    p.x += p.vx * dt;
                    p.y += p.vy * dt;
                    p.life = (p.life - dt * 0.15).max(0.1);
                    bounce_in_area(p, ax, ay, aw, ah);
                }
                // Auto re-explode every 4s in card mode
                if !is_modal && t > 4.0 {
                    state.time = 0.0;
                    let cx = ax + aw * 0.5;
                    let cy = ay + ah * 0.5;
                    let rng = &mut self.rng;
                    let state = &mut self.demos[idx];
                    for p in &mut state.particles {
                        let angle = p.hue / 360.0 * std::f32::consts::TAU;
                        let speed = rng.range(100.0, 500.0);
                        p.x = cx;
                        p.y = cy;
                        p.vx = angle.cos() * speed;
                        p.vy = angle.sin() * speed - 150.0;
                        p.life = 1.0;
                    }
                }
            }
            // Wave Field
            3 => {
                for p in &mut state.particles {
                    let norm_x = (p.x - ax) / aw;
                    let target_y = ay + ah * 0.5
                        + ((norm_x * 10.0 + t * 2.0).sin() * ah * 0.15)
                        + ((norm_x * 25.0 + t * 3.0).sin() * ah * 0.07);
                    p.vy += (target_y - p.y) * 8.0 * dt;
                    if pressed && is_modal {
                        let dx = p.x - mx;
                        let dy = p.y - my;
                        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                        if dist < 150.0 {
                            p.vy += -200.0 * dt * (1.0 - dist / 150.0);
                        }
                    }
                    p.vy *= 0.92;
                    p.y += p.vy * dt;
                }
            }
            // Particle Swarm
            4 => {
                let target_x = if is_modal { mx } else { ax + aw * (0.5 + (t * 0.3).sin() * 0.3) };
                let target_y = if is_modal { my } else { ay + ah * (0.5 + (t * 0.4).cos() * 0.3) };
                let scatter = pressed && is_modal;
                for p in &mut state.particles {
                    if scatter {
                        let dx = p.x - mx;
                        let dy = p.y - my;
                        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                        if dist < 200.0 {
                            p.vx += dx / dist * 500.0 * dt;
                            p.vy += dy / dist * 500.0 * dt;
                        }
                    } else {
                        let dx = target_x - p.x;
                        let dy = target_y - p.y;
                        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                        p.vx += dx / dist * 200.0 * dt;
                        p.vy += dy / dist * 200.0 * dt;
                    }
                    p.vx *= 0.96;
                    p.vy *= 0.96;
                    p.x += p.vx * dt;
                    p.y += p.vy * dt;
                    clamp_in_area(p, ax, ay, aw, ah);
                }
            }
            // N-Body Gravity
            5 => {
                let len = state.particles.len();
                // Collect positions/masses first
                let positions: Vec<(f32, f32, f32)> = state.particles.iter().map(|p| (p.x, p.y, p.size)).collect();
                for i in 0..len {
                    let mut fx = 0.0f32;
                    let mut fy = 0.0f32;
                    for j in 0..len {
                        if i == j { continue; }
                        let dx = positions[j].0 - positions[i].0;
                        let dy = positions[j].1 - positions[i].1;
                        let dist_sq = dx * dx + dy * dy + 100.0;
                        let dist = dist_sq.sqrt();
                        let force = positions[j].2 * 50.0 / dist_sq;
                        fx += dx / dist * force;
                        fy += dy / dist * force;
                    }
                    state.particles[i].vx += fx * dt;
                    state.particles[i].vy += fy * dt;
                }
                for p in &mut state.particles {
                    p.vx *= 0.999;
                    p.vy *= 0.999;
                    p.x += p.vx * dt;
                    p.y += p.vy * dt;
                    soft_clamp(p, ax, ay, aw, ah);
                }
            }
            // Fluid Grid
            6 => {
                for p in &mut state.particles {
                    // vx, vy = rest position
                    let rest_x = p.vx;
                    let rest_y = p.vy;
                    let mut offset_x = 0.0f32;
                    let mut offset_y = 0.0f32;

                    // Mouse ripple
                    let dx = rest_x - mx;
                    let dy = rest_y - my;
                    let dist = (dx * dx + dy * dy).sqrt();
                    let ripple_radius = if is_modal { 150.0 } else { 60.0 };
                    if dist < ripple_radius {
                        let strength = 1.0 - dist / ripple_radius;
                        offset_x += dx / dist.max(1.0) * strength * 20.0;
                        offset_y += dy / dist.max(1.0) * strength * 20.0;
                    }

                    // Ambient wave
                    offset_x += ((rest_x * 0.03 + t * 2.0).sin()) * 5.0;
                    offset_y += ((rest_y * 0.03 + t * 1.5).cos()) * 5.0;

                    p.x = rest_x + offset_x;
                    p.y = rest_y + offset_y;
                }
            }
            // Cloth Sim
            7 => {
                let side = state.values[0] as usize;
                let rest_dx = state.values[1];
                let rest_dy = state.values[2];
                let gravity = 200.0;

                if side == 0 { return; }

                // Verlet integration
                for i in 0..state.particles.len() {
                    let pinned = i < side; // top row is pinned
                    if pinned {
                        // Just sway the pinned row slightly with wind
                        let col = i % side;
                        let base_x = ax + aw * 0.15 + rest_dx * col as f32;
                        let base_y = ay + ah * 0.1;
                        state.particles[i].x = base_x + (t * 1.5 + col as f32 * 0.3).sin() * 3.0;
                        state.particles[i].y = base_y;
                        state.particles[i].vx = state.particles[i].x;
                        state.particles[i].vy = state.particles[i].y;
                        continue;
                    }

                    let p = &mut state.particles[i];
                    let old_x = p.x;
                    let old_y = p.y;
                    let vel_x = (p.x - p.vx) * 0.98;
                    let vel_y = (p.y - p.vy) * 0.98;
                    p.x += vel_x + (t * 2.0).sin() * 0.5; // wind
                    p.y += vel_y + gravity * dt * dt;
                    p.vx = old_x;
                    p.vy = old_y;
                }

                // Constraint solving (3 iterations)
                for _ in 0..3 {
                    let positions: Vec<(f32, f32)> = state.particles.iter().map(|p| (p.x, p.y)).collect();
                    for i in 0..state.particles.len() {
                        let row = i / side;
                        let col = i % side;
                        let pinned = row == 0;

                        // Right neighbor
                        if col + 1 < side {
                            let j = i + 1;
                            let dx = positions[j].0 - positions[i].0;
                            let dy = positions[j].1 - positions[i].1;
                            let dist = (dx * dx + dy * dy).sqrt().max(0.01);
                            let diff = (dist - rest_dx) / dist * 0.5;
                            if !pinned {
                                state.particles[i].x += dx * diff;
                                state.particles[i].y += dy * diff;
                            }
                            let pinned_j = row == 0;
                            if !pinned_j && j < state.particles.len() {
                                state.particles[j].x -= dx * diff;
                                state.particles[j].y -= dy * diff;
                            }
                        }
                        // Bottom neighbor
                        if row + 1 < side {
                            let j = i + side;
                            if j < state.particles.len() {
                                let dx = positions[j].0 - positions[i].0;
                                let dy = positions[j].1 - positions[i].1;
                                let dist = (dx * dx + dy * dy).sqrt().max(0.01);
                                let diff = (dist - rest_dy) / dist * 0.5;
                                if !pinned {
                                    state.particles[i].x += dx * diff;
                                    state.particles[i].y += dy * diff;
                                }
                                if row + 1 != 0 {
                                    state.particles[j].x -= dx * diff;
                                    state.particles[j].y -= dy * diff;
                                }
                            }
                        }
                    }
                }

                // Mouse interaction in modal
                if pressed && is_modal {
                    for p in &mut state.particles {
                        let dx = p.x - mx;
                        let dy = p.y - my;
                        let dist = (dx * dx + dy * dy).sqrt();
                        if dist < 40.0 {
                            p.x = mx + dx / dist.max(1.0) * 40.0;
                            p.y = my + dy / dist.max(1.0) * 40.0;
                        }
                    }
                }
            }
            // Fireworks
            8 => {
                let rng = &mut self.rng;
                let state = &mut self.demos[idx];
                let mut new_particles = Vec::new();

                for p in state.particles.iter_mut() {
                    p.vy += 150.0 * dt;
                    p.vx *= 0.99;
                    p.vy *= 0.99;
                    p.x += p.vx * dt;
                    p.y += p.vy * dt;
                    p.life -= dt * 0.4;

                    // Rocket explodes at peak
                    if p.size > 3.5 && p.vy > -10.0 && p.life > 0.5 {
                        let explode_count = if is_modal { 80 } else { 15 };
                        let base_hue = rng.range(0.0, 360.0);
                        for j in 0..explode_count {
                            let angle = j as f32 / explode_count as f32 * std::f32::consts::TAU;
                            let speed = rng.range(60.0, 200.0);
                            new_particles.push(Particle::new_full(
                                p.x, p.y,
                                angle.cos() * speed + p.vx * 0.3,
                                angle.sin() * speed + p.vy * 0.3,
                                1.0,
                                rng.range(1.5, 3.0),
                                base_hue + rng.range(-20.0, 20.0),
                            ));
                        }
                        p.life = 0.0;
                    }
                }

                state.particles.retain(|p| p.life > 0.0 && p.y < ay + ah + 20.0);
                state.particles.extend(new_particles);

                // Auto-launch new rockets
                let max_particles = if is_modal { 3000 } else { 300 };
                if state.particles.len() < max_particles / 2 {
                    let count = if is_modal { 3 } else { 1 };
                    for _ in 0..count {
                        let px = ax + rng.range(aw * 0.1, aw * 0.9);
                        state.particles.push(Particle::new_full(px, ay + ah, rng.range(-20.0, 20.0), -rng.range(200.0, 400.0), 1.0, 4.0, rng.range(0.0, 360.0)));
                    }
                }
            }
            // Galaxy
            9 => {
                let cx = ax + aw * 0.5;
                let cy = ay + ah * 0.5;
                for p in &mut state.particles {
                    let dx = p.x - cx;
                    let dy = p.y - cy;
                    let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                    let angle = dy.atan2(dx);
                    let orbital_speed = 80.0 / (dist.sqrt().max(1.0));
                    p.x += (-angle.sin()) * orbital_speed * dt * 30.0;
                    p.y += (angle.cos()) * orbital_speed * dt * 30.0 * 0.6;
                    // Slight pull inward
                    p.x += (cx - p.x) * 0.1 * dt;
                    p.y += (cy - p.y) * 0.1 * dt;
                }
            }
            // Spring Balls
            10 => {
                if is_modal && pressed {
                    // Find nearest ball and drag
                    let mut best = 0;
                    let mut best_dist = f32::MAX;
                    for (i, p) in state.particles.iter().enumerate() {
                        let d = (p.x - mx).powi(2) + (p.y - my).powi(2);
                        if d < best_dist { best_dist = d; best = i; }
                    }
                    if best_dist < 50.0 * 50.0 {
                        state.particles[best].x = mx;
                        state.particles[best].y = my;
                    }
                }
                // Spring connections to neighbors
                let positions: Vec<(f32, f32)> = state.particles.iter().map(|p| (p.x, p.y)).collect();
                let rest_len = aw / 6.0;
                for i in 0..state.particles.len() {
                    if i + 1 < state.particles.len() {
                        let dx = positions[i + 1].0 - positions[i].0;
                        let dy = positions[i + 1].1 - positions[i].1;
                        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                        let force = (dist - rest_len) * 3.0;
                        state.particles[i].vx += dx / dist * force * dt * 60.0;
                        state.particles[i].vy += dy / dist * force * dt * 60.0;
                        state.particles[i + 1].vx -= dx / dist * force * dt * 60.0;
                        state.particles[i + 1].vy -= dy / dist * force * dt * 60.0;
                    }
                    // Center gravity
                    state.particles[i].vy += 100.0 * dt;
                    state.particles[i].vx *= 0.95;
                    state.particles[i].vy *= 0.95;
                    state.particles[i].x += state.particles[i].vx * dt;
                    state.particles[i].y += state.particles[i].vy * dt;
                    bounce_in_area(&mut state.particles[i], ax, ay, aw, ah);
                }
            }
            // macOS Dock — just updates happen at render time using mouse position
            11 => {}
            // Parallax Layers
            12 => {
                // Positions shift in render based on mouse offset
            }
            // Card Tilt 3D — computed at render time
            13 => {}
            // Elastic Cursor
            14 => {
                if let Some(p) = state.particles.first_mut() {
                    let target_x = if is_modal { mx } else { ax + aw * 0.5 + (t * 1.2).sin() * aw * 0.3 };
                    let target_y = if is_modal { my } else { ay + ah * 0.5 + (t * 0.9).cos() * ah * 0.3 };
                    let spring = 6.0;
                    let damp = 0.85;
                    p.vx += (target_x - p.x) * spring * dt * 60.0;
                    p.vy += (target_y - p.y) * spring * dt * 60.0;
                    p.vx *= damp;
                    p.vy *= damp;
                    p.x += p.vx * dt;
                    p.y += p.vy * dt;
                }
            }
            // Toggle Switch
            15 => {
                let target = if ((t / 2.0) as u32) % 2 == 0 { 0.0 } else { 1.0 };
                state.values[0] += (target - state.values[0]) * 8.0 * dt;
            }
            // Morph Button
            16 => {
                let cycle = (t / 3.0) as u32 % 3;
                let target_phase = cycle as f32;
                state.values[0] += (target_phase - state.values[0]) * 4.0 * dt;
                state.values[1] = t; // for spinner rotation
            }
            // Loading Wave
            17 => {
                for i in 0..12 {
                    state.values[i] = ((t * 4.0 + i as f32 * 0.5).sin() * 0.5 + 0.5).max(0.1);
                }
            }
            // Stagger Reveal
            18 => {
                let cycle_t = t % 4.0;
                for i in 0..8 {
                    let delay = i as f32 * 0.15;
                    if cycle_t < 2.0 {
                        state.values[i] = ((cycle_t - delay) * 3.0).clamp(0.0, 1.0);
                    } else {
                        state.values[i] = (1.0 - (cycle_t - 2.0 - delay) * 3.0).clamp(0.0, 1.0);
                    }
                }
            }
            // Ripple Click
            19 => {
                // Auto-generate ripples for card mode
                if !is_modal && t.fract() < dt {
                    let cx = ax + aw * 0.5 + (t * 1.7).sin() * aw * 0.2;
                    let cy = ay + ah * 0.5 + (t * 1.3).cos() * ah * 0.2;
                    state.particles.push(Particle::new_full(cx, cy, 0.0, 0.0, 1.0, 0.0, 200.0));
                }
                for p in &mut state.particles {
                    p.size += 200.0 * dt; // expanding radius
                    p.life -= dt * 0.8;
                }
                state.particles.retain(|p| p.life > 0.0);
            }
            // Live Bar Chart
            20 => {
                // Smoothly animate values toward targets
                for i in 0..10 {
                    state.values[i] += (state.values[10 + i] - state.values[i]) * 5.0 * dt;
                }
                // New random targets periodically
                if (t * 2.0).fract() < dt * 2.0 {
                    let idx_bar = (self.rng.next() % 10) as usize;
                    state.values[10 + idx_bar] = self.rng.range(0.1, 1.0);
                }
            }
            // Sparkline
            21 => {
                // Shift values left and add new one periodically
                if (t * 3.0).fract() < dt * 3.0 {
                    for i in 0..31 { state.values[i] = state.values[i + 1]; }
                    state.values[31] = self.rng.range(0.1, 0.9);
                }
            }
            // Donut Chart
            22 => {
                state.values[5] = (t * 0.5).min(1.0); // anim in
                // Slowly shift values
                if (t * 0.5).fract() < dt * 0.5 {
                    let idx_seg = (self.rng.next() % 5) as usize;
                    state.values[idx_seg] = self.rng.range(0.1, 0.5);
                    let total: f32 = state.values[0..5].iter().sum();
                    for i in 0..5 { state.values[i] /= total; }
                }
            }
            // Heatmap — computed at render time
            23 => {}
            // Network Graph
            24 => {
                let len = state.particles.len();
                let positions: Vec<(f32, f32)> = state.particles.iter().map(|p| (p.x, p.y)).collect();

                for i in 0..len {
                    let mut fx = 0.0f32;
                    let mut fy = 0.0f32;
                    for j in 0..len {
                        if i == j { continue; }
                        let dx = positions[j].0 - positions[i].0;
                        let dy = positions[j].1 - positions[i].1;
                        let dist = (dx * dx + dy * dy).sqrt().max(1.0);

                        // Repulsion
                        let repel = -3000.0 / (dist * dist + 100.0);
                        fx += dx / dist * repel;
                        fy += dy / dist * repel;

                        // Attraction for connected nodes (neighbors within 3)
                        if (i as i32 - j as i32).unsigned_abs() <= 3 || (i + j) % 7 == 0 {
                            let attract = (dist - 60.0) * 0.5;
                            fx += dx / dist * attract;
                            fy += dy / dist * attract;
                        }
                    }

                    // Center pull
                    let cx = ax + aw * 0.5;
                    let cy = ay + ah * 0.5;
                    fx += (cx - positions[i].0) * 0.3;
                    fy += (cy - positions[i].1) * 0.3;

                    state.particles[i].vx += fx * dt;
                    state.particles[i].vy += fy * dt;
                }

                for p in &mut state.particles {
                    p.vx *= 0.9;
                    p.vy *= 0.9;
                    p.x += p.vx * dt;
                    p.y += p.vy * dt;
                    soft_clamp(p, ax, ay, aw, ah);
                }
            }
            // Matrix Rain
            25 => {
                for p in &mut state.particles {
                    p.y += p.vy * dt; // vy = fall speed
                    p.life -= dt * 0.15;
                    if p.y > ay + ah {
                        p.y = ay - self.rng.range(0.0, 30.0);
                        p.life = 1.0;
                    }
                }
            }
            // Starfield
            26 => {
                let cx = ax + aw * 0.5;
                let cy = ay + ah * 0.5;
                for p in &mut state.particles {
                    let speed = 50.0 + p.life * 200.0;
                    p.x += p.vx * speed * dt;
                    p.y += p.vy * speed * dt;
                    p.life += dt * 0.3;
                    // Reset if out of bounds
                    if p.x < ax - 10.0 || p.x > ax + aw + 10.0 || p.y < ay - 10.0 || p.y > ay + ah + 10.0 {
                        p.x = cx + self.rng.range(-20.0, 20.0);
                        p.y = cy + self.rng.range(-20.0, 20.0);
                        let angle = self.rng.range(0.0, std::f32::consts::TAU);
                        p.vx = angle.cos();
                        p.vy = angle.sin();
                        p.life = 0.0;
                    }
                }
            }
            // Aurora
            27 => {
                for p in &mut state.particles {
                    let band = p.vy; // stored band index
                    let base_y = ay + ah * (0.2 + band * 0.12);
                    let wave = ((p.vx * 0.005 + t * 0.8 + band * 1.5).sin() * ah * 0.08)
                        + ((p.vx * 0.01 + t * 1.2).cos() * ah * 0.04);
                    p.y = base_y + wave;
                    // Gentle horizontal drift
                    p.x += (t * 0.3 + band * 0.5).sin() * 10.0 * dt;
                    if p.x > ax + aw { p.x = ax; }
                    if p.x < ax { p.x = ax + aw; }
                }
            }
            // Rain
            28 => {
                for p in &mut state.particles {
                    p.x += p.vx * dt;
                    p.y += p.vy * dt;
                    if p.y > ay + ah {
                        p.y = ay - self.rng.range(0.0, 20.0);
                        p.x = ax + self.rng.range(0.0, aw);
                        p.vy = self.rng.range(200.0, 500.0);
                    }
                }
            }
            // Fireflies
            29 => {
                for p in &mut state.particles {
                    p.vx += self.rng.range(-30.0, 30.0) * dt;
                    p.vy += self.rng.range(-30.0, 30.0) * dt;
                    p.vx *= 0.95;
                    p.vy *= 0.95;
                    p.x += p.vx * dt;
                    p.y += p.vy * dt;
                    p.life = ((t * 2.0 + p.hue * 0.1).sin() * 0.5 + 0.5).max(0.15);
                    soft_clamp(p, ax, ay, aw, ah);
                }
            }
            _ => {}
        }
    }

    // ------ Render a single demo into rects ------
    fn render_demo(&mut self, idx: usize, is_modal: bool, rects: &mut Vec<RectInstance>) {
        let (ax, ay, aw, ah) = self.demo_render_area(idx, is_modal);
        let t = self.demos[idx].time;

        match idx {
            // Particle Gravity / Orbit / Explosion / Wave / Swarm
            0 | 1 | 2 | 3 | 4 => {
                for p in &self.demos[idx].particles {
                    let speed = (p.vx * p.vx + p.vy * p.vy).sqrt();
                    let col = speed_color(p.hue, speed, p.life * 0.85);
                    let r = p.size * (if is_modal { 1.0 } else { 0.6 });
                    rects.push(circle_rect(p.x, p.y, r, col));
                }
            }
            // N-Body Gravity
            5 => {
                for p in &self.demos[idx].particles {
                    let speed = (p.vx * p.vx + p.vy * p.vy).sqrt();
                    let col = speed_color(p.hue, speed, 0.9);
                    let r = p.size * (if is_modal { 1.0 } else { 0.6 });
                    // Glow
                    rects.push(RectInstance {
                        rect: [p.x - r * 2.0, p.y - r * 2.0, r * 4.0, r * 4.0],
                        fill_color: col.with_alpha(0.15).to_array(),
                        corner_radii: [r * 2.0; 4],
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                        shadow_color: [0.0; 4],
                        shadow_offset: [0.0; 2],
                        shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                    });
                    rects.push(circle_rect(p.x, p.y, r, col));
                }
            }
            // Fluid Grid
            6 => {
                for p in &self.demos[idx].particles {
                    let col = hue_to_color(p.hue, 0.8, 0.8, 0.9);
                    let r = p.size * (if is_modal { 1.0 } else { 0.5 });
                    rects.push(circle_rect(p.x, p.y, r, col));
                }
            }
            // Cloth Sim
            7 => {
                let side = self.demos[idx].values[0] as usize;
                if side == 0 { return; }

                // Draw connections as thin rects
                let particles = &self.demos[idx].particles;
                for i in 0..particles.len() {
                    let row = i / side;
                    let col = i % side;
                    // Right connection
                    if col + 1 < side && i + 1 < particles.len() {
                        let p1 = &particles[i];
                        let p2 = &particles[i + 1];
                        let dx = p2.x - p1.x;
                        let dy = p2.y - p1.y;
                        let len = (dx * dx + dy * dy).sqrt();
                        let mx = (p1.x + p2.x) * 0.5;
                        let my = (p1.y + p2.y) * 0.5;
                        let hue = (row as f32 / side as f32) * 240.0;
                        let col_c = hue_to_color(hue, 0.6, 0.7, 0.4);
                        rects.push(flat_rect(mx - len * 0.5, my - 0.5, len, 1.0, 0.5, col_c));
                    }
                    // Down connection
                    if row + 1 < side && i + side < particles.len() {
                        let p1 = &particles[i];
                        let p2 = &particles[i + side];
                        let dx = p2.x - p1.x;
                        let dy = p2.y - p1.y;
                        let len = (dx * dx + dy * dy).sqrt();
                        let mx = (p1.x + p2.x) * 0.5;
                        let my = (p1.y + p2.y) * 0.5;
                        let hue = (row as f32 / side as f32) * 240.0;
                        let col_c = hue_to_color(hue, 0.6, 0.7, 0.4);
                        rects.push(flat_rect(mx - 0.5, my - len * 0.5, 1.0, len, 0.5, col_c));
                    }
                }

                // Draw nodes
                for p in particles {
                    let col = hue_to_color(p.hue, 0.7, 0.9, 0.9);
                    let r = if is_modal { 3.0 } else { 2.0 };
                    rects.push(circle_rect(p.x, p.y, r, col));
                }
            }
            // Fireworks
            8 => {
                for p in &self.demos[idx].particles {
                    let alpha = p.life.clamp(0.0, 1.0);
                    let col = hue_to_color(p.hue, 0.9, 1.0, alpha * 0.9);
                    let r = p.size * alpha * (if is_modal { 1.0 } else { 0.5 });
                    if r > 0.5 {
                        // Trail glow for large particles
                        if p.size > 3.5 {
                            rects.push(circle_rect(p.x, p.y, r * 2.0, col.with_alpha(alpha * 0.2)));
                        }
                        rects.push(circle_rect(p.x, p.y, r, col));
                    }
                }
            }
            // Galaxy
            9 => {
                for p in &self.demos[idx].particles {
                    let col = hue_to_color(p.hue, 0.5, 0.9, 0.8);
                    let r = p.size * (if is_modal { 1.0 } else { 0.5 });
                    rects.push(circle_rect(p.x, p.y, r, col));
                }
            }
            // Spring Balls
            10 => {
                let particles = &self.demos[idx].particles;
                // Draw springs
                for i in 0..particles.len().saturating_sub(1) {
                    let p1 = &particles[i];
                    let p2 = &particles[i + 1];
                    let dx = p2.x - p1.x;
                    let dy = p2.y - p1.y;
                    let len = (dx * dx + dy * dy).sqrt();
                    let mx = (p1.x + p2.x) * 0.5;
                    let my = (p1.y + p2.y) * 0.5;
                    rects.push(flat_rect(mx - len * 0.5, my - 1.0, len, 2.0, 1.0, c(CYAN).with_alpha(0.4)));
                }
                // Draw balls
                for p in particles {
                    let col = hue_to_color(p.hue, 0.8, 0.9, 0.95);
                    let r = if is_modal { 18.0 } else { 8.0 };
                    rects.push(shadow_rect(p.x - r, p.y - r, r * 2.0, r * 2.0, r, col, Color::BLACK.with_alpha(0.3), [0.0, 3.0], [6.0, 0.0]));
                }
            }
            // macOS Dock
            11 => {
                let icon_count = self.demos[idx].particles.len();
                if icon_count == 0 { return; }
                let base_size = if is_modal { 48.0 } else { 20.0 };
                let max_size = base_size * 1.8;
                let spacing = if is_modal { 60.0 } else { 26.0 };
                let total_w = icon_count as f32 * spacing;
                let dock_x = ax + (aw - total_w) * 0.5;
                let dock_y = ay + ah - base_size - (if is_modal { 30.0 } else { 10.0 });

                // Dock background
                rects.push(flat_rect(dock_x - 12.0, dock_y - 8.0, total_w + 24.0, base_size + 16.0, 12.0, c(BG2).with_alpha(0.8)));

                for i in 0..icon_count {
                    let center_x = dock_x + i as f32 * spacing + spacing * 0.5;
                    let dist = (self.mouse_x - center_x).abs();
                    let magnify_range = if is_modal { 150.0 } else { 60.0 };
                    let scale = if dist < magnify_range {
                        1.0 + 0.8 * (1.0 - dist / magnify_range).powi(2)
                    } else {
                        1.0
                    };
                    let size = base_size * scale;
                    let cy = dock_y + base_size * 0.5 - (size - base_size);
                    let col = hue_to_color(i as f32 * 30.0, 0.7, 0.85, 0.95);
                    rects.push(shadow_rect(
                        center_x - size * 0.5, cy - size * 0.5, size, size,
                        size * 0.22, col, Color::BLACK.with_alpha(0.2), [0.0, 2.0], [4.0, 0.0],
                    ));
                }
            }
            // Parallax Layers
            12 => {
                let norm_mx = (self.mouse_x - ax) / aw - 0.5;
                let norm_my = (self.mouse_y - ay) / ah - 0.5;
                for p in &self.demos[idx].particles {
                    let depth = p.life; // 0..1 layer depth
                    let offset_x = norm_mx * depth * (if is_modal { 80.0 } else { 30.0 });
                    let offset_y = norm_my * depth * (if is_modal { 60.0 } else { 20.0 });
                    let col = hue_to_color(p.hue, 0.5, 0.7 + depth * 0.3, 0.3 + depth * 0.6);
                    let r = p.size * (if is_modal { 1.0 } else { 0.5 });
                    rects.push(circle_rect(p.vx + offset_x, p.vy + offset_y, r, col));
                }
            }
            // Card Tilt 3D
            13 => {
                let card_w = aw * 0.6;
                let card_h = ah * 0.7;
                let card_x = ax + (aw - card_w) * 0.5;
                let card_y = ay + (ah - card_h) * 0.5;

                let norm_mx = ((self.mouse_x - card_x) / card_w - 0.5).clamp(-1.0, 1.0);
                let norm_my = ((self.mouse_y - card_y) / card_h - 0.5).clamp(-1.0, 1.0);

                let shadow_x = norm_mx * -15.0;
                let shadow_y = norm_my * -15.0;
                let shadow_blur = 15.0 + (norm_mx.abs() + norm_my.abs()) * 10.0;

                // Card with dynamic shadow
                rects.push(RectInstance {
                    rect: [card_x, card_y, card_w, card_h],
                    fill_color: c(BG2).to_array(),
                    corner_radii: [12.0; 4],
                    border_color: c(BORDER).to_array(),
                    border_width: 1.0,
                    gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                    shadow_color: Color::BLACK.with_alpha(0.5).to_array(),
                    shadow_offset: [shadow_x, shadow_y],
                    shadow_params: [shadow_blur, 2.0],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                });

                // Highlight spot (follows mouse)
                let hl_x = card_x + (norm_mx * 0.5 + 0.5) * card_w;
                let hl_y = card_y + (norm_my * 0.5 + 0.5) * card_h;
                let hl_r = card_w.min(card_h) * 0.3;
                rects.push(circle_rect(hl_x, hl_y, hl_r, Color::WHITE.with_alpha(0.06)));

                // Inner content bars
                for i in 0..4 {
                    let bar_y = card_y + 20.0 + i as f32 * (if is_modal { 30.0 } else { 15.0 });
                    let bar_w = card_w * (0.4 + (i as f32 * 0.15));
                    rects.push(flat_rect(card_x + 16.0, bar_y, bar_w, if is_modal { 12.0 } else { 6.0 }, 3.0, c(BG3)));
                }
            }
            // Elastic Cursor
            14 => {
                if let Some(p) = self.demos[idx].particles.first() {
                    let r = if is_modal { 25.0 } else { 10.0 };
                    // Glow
                    rects.push(circle_rect(p.x, p.y, r * 3.0, c(PRIMARY).with_alpha(0.08)));
                    rects.push(circle_rect(p.x, p.y, r * 2.0, c(PRIMARY).with_alpha(0.15)));
                    // Main blob
                    rects.push(shadow_rect(
                        p.x - r, p.y - r, r * 2.0, r * 2.0, r,
                        c(PRIMARY), c(PRIMARY).with_alpha(0.4), [0.0, 0.0], [12.0, 4.0],
                    ));
                    // Trail dots
                    let speed = (p.vx * p.vx + p.vy * p.vy).sqrt();
                    if speed > 5.0 {
                        for i in 1..6 {
                            let frac = i as f32 / 6.0;
                            let tr = r * (1.0 - frac * 0.7);
                            let tx = p.x - p.vx * frac * 0.05;
                            let ty = p.y - p.vy * frac * 0.05;
                            rects.push(circle_rect(tx, ty, tr, c(PRIMARY).with_alpha(0.3 * (1.0 - frac))));
                        }
                    }
                }
            }
            // Toggle Switch
            15 => {
                let anim = self.demos[idx].values[0];
                let sw_w = if is_modal { 80.0 } else { 40.0 };
                let sw_h = if is_modal { 40.0 } else { 20.0 };
                let sw_x = ax + (aw - sw_w) * 0.5;
                let sw_y = ay + (ah - sw_h) * 0.5;
                let bg_col = c(BG3).lerp(c(SUCCESS), anim);

                // Track
                rects.push(flat_rect(sw_x, sw_y, sw_w, sw_h, sw_h * 0.5, bg_col));

                // Knob
                let knob_r = sw_h * 0.4;
                let knob_x = sw_x + knob_r + 2.0 + anim * (sw_w - knob_r * 2.0 - 4.0) + knob_r;
                let knob_y = sw_y + sw_h * 0.5;
                rects.push(shadow_rect(
                    knob_x - knob_r, knob_y - knob_r, knob_r * 2.0, knob_r * 2.0, knob_r,
                    Color::WHITE, Color::BLACK.with_alpha(0.2), [0.0, 1.0], [3.0, 0.0],
                ));
            }
            // Morph Button
            16 => {
                let phase = self.demos[idx].values[0];
                let spin_t = self.demos[idx].values[1];
                let btn_w = if is_modal { 180.0 } else { 80.0 };
                let btn_h = if is_modal { 48.0 } else { 24.0 };
                let btn_x = ax + (aw - btn_w) * 0.5;
                let btn_y = ay + (ah - btn_h) * 0.5;

                if phase < 0.5 {
                    // Submit button
                    rects.push(flat_rect(btn_x, btn_y, btn_w, btn_h, btn_h * 0.5, c(PRIMARY)));
                } else if phase < 1.5 {
                    // Morphing to circle spinner
                    let morph = (phase - 0.5).min(1.0);
                    let w = btn_w * (1.0 - morph * 0.7);
                    let h = btn_h;
                    let x = ax + (aw - w) * 0.5;
                    rects.push(flat_rect(x, btn_y, w, h, h * 0.5, c(PRIMARY)));
                    // Spinner dots
                    let dot_count = 8;
                    let center_x = ax + aw * 0.5;
                    let center_y = btn_y + btn_h * 0.5;
                    let radius = btn_h * 0.25;
                    for i in 0..dot_count {
                        let angle = i as f32 / dot_count as f32 * std::f32::consts::TAU + spin_t * 5.0;
                        let alpha = (((i as f32 / dot_count as f32) - (spin_t * 2.0).fract()).fract() * 2.0).min(1.0) * morph;
                        let dx = angle.cos() * radius;
                        let dy = angle.sin() * radius;
                        let dr = if is_modal { 3.0 } else { 1.5 };
                        rects.push(circle_rect(center_x + dx, center_y + dy, dr, Color::WHITE.with_alpha(alpha)));
                    }
                } else {
                    // Done — green check
                    rects.push(flat_rect(btn_x, btn_y, btn_w, btn_h, btn_h * 0.5, c(SUCCESS)));
                }
            }
            // Loading Wave
            17 => {
                let bar_count = 12;
                let bar_w = aw / (bar_count as f32 * 2.0);
                let max_h = ah * 0.7;
                let base_y = ay + ah * 0.85;
                for i in 0..bar_count {
                    let val = self.demos[idx].values[i];
                    let h = val * max_h;
                    let x = ax + (aw - bar_count as f32 * bar_w * 1.8) * 0.5 + i as f32 * bar_w * 1.8;
                    let hue = i as f32 / bar_count as f32 * 240.0 + 180.0;
                    let col = hue_to_color(hue, 0.7, 0.9, 0.9);
                    rects.push(flat_rect(x, base_y - h, bar_w, h, bar_w * 0.3, col));
                }
            }
            // Stagger Reveal
            18 => {
                let item_count = 8;
                let item_h = ah / (item_count as f32 + 1.0);
                let item_w = aw * 0.8;
                let ox = ax + (aw - item_w) * 0.5;
                for i in 0..item_count {
                    let alpha = self.demos[idx].values[i];
                    if alpha < 0.01 { continue; }
                    let slide = (1.0 - alpha) * 30.0;
                    let y = ay + (i as f32 + 0.5) * item_h + slide;
                    let hue = i as f32 / item_count as f32 * 200.0 + 180.0;
                    let col = hue_to_color(hue, 0.5, 0.7, alpha * 0.9);
                    let h = item_h * 0.7;
                    rects.push(flat_rect(ox, y, item_w, h, 4.0, col));
                    // Inner bar
                    rects.push(flat_rect(ox + 8.0, y + h * 0.3, item_w * (0.3 + i as f32 * 0.08), h * 0.4, 2.0, Color::WHITE.with_alpha(alpha * 0.2)));
                }
            }
            // Ripple Click
            19 => {
                // Base surface
                rects.push(flat_rect(ax + 4.0, ay + 4.0, aw - 8.0, ah - 8.0, 8.0, c(BG2)));
                for p in &self.demos[idx].particles {
                    let alpha = p.life.clamp(0.0, 1.0) * 0.4;
                    let r = p.size;
                    if r > 0.5 {
                        rects.push(bordered_rect(
                            p.x - r, p.y - r, r * 2.0, r * 2.0, r,
                            Color::WHITE.with_alpha(alpha * 0.1),
                            Color::WHITE.with_alpha(alpha * 0.5),
                            1.5,
                        ));
                    }
                }
            }
            // Live Bar Chart
            20 => {
                let bar_count = 10;
                let bar_w = aw / (bar_count as f32 * 1.5 + 0.5);
                let gap = bar_w * 0.5;
                let max_h = ah * 0.85;
                let base_y = ay + ah;
                for i in 0..bar_count {
                    let val = self.demos[idx].values[i];
                    let h = val * max_h;
                    let x = ax + gap * 0.5 + i as f32 * (bar_w + gap);
                    let hue = i as f32 / bar_count as f32 * 280.0 + 180.0;
                    let col = hue_to_color(hue, 0.7, 0.85, 0.9);
                    rects.push(flat_rect(x, base_y - h, bar_w, h, 3.0, col));
                }
            }
            // Sparkline
            21 => {
                let points = 32;
                let step = aw / (points as f32 - 1.0);
                let dot_r = if is_modal { 3.0 } else { 1.5 };
                // Draw line segments as thin rects + dots
                for i in 0..points {
                    let val = self.demos[idx].values[i];
                    let x = ax + i as f32 * step;
                    let y = ay + ah * (1.0 - val);
                    let hue = 200.0 + val * 60.0;
                    let col = hue_to_color(hue, 0.7, 0.9, 0.9);
                    rects.push(circle_rect(x, y, dot_r, col));

                    if i + 1 < points {
                        let next_val = self.demos[idx].values[i + 1];
                        let nx = ax + (i + 1) as f32 * step;
                        let ny = ay + ah * (1.0 - next_val);
                        // Simple line approximation as thin rect
                        let mx = (x + nx) * 0.5;
                        let my = (y + ny) * 0.5;
                        let len = ((nx - x).powi(2) + (ny - y).powi(2)).sqrt();
                        rects.push(flat_rect(mx - len * 0.5, my - 0.5, len, 1.5, 0.5, col.with_alpha(0.5)));
                    }
                }
            }
            // Donut Chart
            22 => {
                let cx = ax + aw * 0.5;
                let cy = ay + ah * 0.5;
                let outer_r = (aw.min(ah) * 0.4).max(20.0);
                let inner_r = outer_r * 0.55;
                let anim_prog = self.demos[idx].values[5];
                let segment_count = 5;
                let colors = [c(PRIMARY), c(SUCCESS), c(WARNING), c(PURPLE), c(CYAN)];

                // Draw segments as arcs approximated by small rects
                let total_steps = if is_modal { 120 } else { 40 };
                let mut angle_start = 0.0f32;
                for seg in 0..segment_count {
                    let frac = self.demos[idx].values[seg] * anim_prog;
                    let angle_end = angle_start + frac * std::f32::consts::TAU;
                    let steps = ((frac * total_steps as f32) as usize).max(1);
                    let step_angle = (angle_end - angle_start) / steps as f32;
                    let col = colors[seg % colors.len()];
                    for s in 0..steps {
                        let a = angle_start + s as f32 * step_angle;
                        let mid_r = (outer_r + inner_r) * 0.5;
                        let px = cx + a.cos() * mid_r;
                        let py = cy + a.sin() * mid_r;
                        let dot_r = (outer_r - inner_r) * 0.5;
                        rects.push(circle_rect(px, py, dot_r, col));
                    }
                    angle_start = angle_end;
                }
            }
            // Heatmap
            23 => {
                let grid = 20;
                let cell_w = aw / grid as f32;
                let cell_h = ah / grid as f32;
                for row in 0..grid {
                    for col in 0..grid {
                        let val = ((col as f32 * 0.3 + t * 1.5).sin() * 0.5 + 0.5)
                            * ((row as f32 * 0.3 + t * 1.2).cos() * 0.5 + 0.5);
                        let hue = val * 240.0; // blue→red
                        let col_c = hue_to_color(hue, 0.8, 0.3 + val * 0.7, 0.9);
                        let x = ax + col as f32 * cell_w;
                        let y = ay + row as f32 * cell_h;
                        rects.push(flat_rect(x + 0.5, y + 0.5, cell_w - 1.0, cell_h - 1.0, 1.0, col_c));
                    }
                }
            }
            // Network Graph
            24 => {
                let particles = &self.demos[idx].particles;
                let len = particles.len();
                // Draw edges
                for i in 0..len {
                    for j in (i + 1)..len {
                        if (i as i32 - j as i32).unsigned_abs() <= 3 || (i + j) % 7 == 0 {
                            let p1 = &particles[i];
                            let p2 = &particles[j];
                            let dx = p2.x - p1.x;
                            let dy = p2.y - p1.y;
                            let dist = (dx * dx + dy * dy).sqrt();
                            if dist < (if is_modal { 200.0 } else { 80.0 }) {
                                let alpha = (1.0 - dist / (if is_modal { 200.0 } else { 80.0 })) * 0.3;
                                let mx = (p1.x + p2.x) * 0.5;
                                let my = (p1.y + p2.y) * 0.5;
                                rects.push(flat_rect(mx - dist * 0.5, my - 0.5, dist, 1.0, 0.5, c(CYAN).with_alpha(alpha)));
                            }
                        }
                    }
                }
                // Draw nodes
                for p in particles {
                    let col = hue_to_color(p.hue, 0.7, 0.85, 0.9);
                    let r = p.size * (if is_modal { 1.0 } else { 0.5 });
                    rects.push(circle_rect(p.x, p.y, r, col));
                }
            }
            // Matrix Rain
            25 => {
                for p in &self.demos[idx].particles {
                    let alpha = p.life.clamp(0.2, 1.0);
                    let col = hue_to_color(p.hue, 0.9, 0.9, alpha);
                    let r = if is_modal { 4.0 } else { 2.0 };
                    let h = if is_modal { 12.0 } else { 6.0 };
                    rects.push(flat_rect(p.x, p.y, r, h, 1.0, col));
                    // Leading bright pixel
                    rects.push(flat_rect(p.x, p.y, r, r, 1.0, Color::WHITE.with_alpha(alpha * 0.6)));
                }
            }
            // Starfield
            26 => {
                for p in &self.demos[idx].particles {
                    let brightness = p.life.clamp(0.0, 1.0);
                    let r = p.size * brightness * (if is_modal { 1.5 } else { 0.7 });
                    if r > 0.3 {
                        let col = hue_to_color(p.hue, 0.2, 0.8 + brightness * 0.2, brightness * 0.9);
                        rects.push(circle_rect(p.x, p.y, r, col));
                    }
                }
            }
            // Aurora
            27 => {
                for p in &self.demos[idx].particles {
                    let col = hue_to_color(p.hue, 0.6, 0.8, 0.25);
                    let w = if is_modal { 8.0 } else { 4.0 };
                    let h = if is_modal { 30.0 } else { 12.0 };
                    rects.push(flat_rect(p.x - w * 0.5, p.y - h * 0.5, w, h, 3.0, col));
                }
            }
            // Rain
            28 => {
                for p in &self.demos[idx].particles {
                    let col = hue_to_color(p.hue, 0.4, 0.7, 0.6);
                    let w = p.size * (if is_modal { 1.0 } else { 0.5 });
                    let h = p.size * (if is_modal { 8.0 } else { 4.0 });
                    rects.push(flat_rect(p.x, p.y, w, h, w * 0.5, col));
                }
            }
            // Fireflies
            29 => {
                for p in &self.demos[idx].particles {
                    let glow = p.life;
                    let r = p.size * glow * (if is_modal { 1.0 } else { 0.5 });
                    if r > 0.5 {
                        let col = hue_to_color(p.hue, 0.8, 1.0, glow * 0.9);
                        // Glow halo
                        rects.push(circle_rect(p.x, p.y, r * 3.0, col.with_alpha(glow * 0.1)));
                        rects.push(circle_rect(p.x, p.y, r, col));
                    }
                }
            }
            _ => {}
        }
    }

    // ------ Hit test for grid cards ------
    fn hit_card(&self, x: f32, y: f32) -> Option<usize> {
        for i in 0..DEMO_COUNT {
            let (cx, cy, cw, ch) = self.card_rect(i);
            if x >= cx && x <= cx + cw && y >= cy && y <= cy + ch + 24.0 {
                return Some(i);
            }
        }
        None
    }
}

// ---------------------------------------------------------------------------
// Helper physics
// ---------------------------------------------------------------------------
fn bounce_in_area(p: &mut Particle, ax: f32, ay: f32, aw: f32, ah: f32) {
    let r = p.size;
    if p.x < ax + r { p.x = ax + r; p.vx = p.vx.abs() * 0.7; }
    if p.x > ax + aw - r { p.x = ax + aw - r; p.vx = -p.vx.abs() * 0.7; }
    if p.y < ay + r { p.y = ay + r; p.vy = p.vy.abs() * 0.7; }
    if p.y > ay + ah - r { p.y = ay + ah - r; p.vy = -p.vy.abs() * 0.7; }
}

fn clamp_in_area(p: &mut Particle, ax: f32, ay: f32, aw: f32, ah: f32) {
    p.x = p.x.clamp(ax, ax + aw);
    p.y = p.y.clamp(ay, ay + ah);
}

fn soft_clamp(p: &mut Particle, ax: f32, ay: f32, aw: f32, ah: f32) {
    let cx = ax + aw * 0.5;
    let cy = ay + ah * 0.5;
    let margin = 20.0;
    if p.x < ax - margin || p.x > ax + aw + margin || p.y < ay - margin || p.y > ay + ah + margin {
        p.vx += (cx - p.x) * 0.5;
        p.vy += (cy - p.y) * 0.5;
    }
}

// ---------------------------------------------------------------------------
// ApplicationHandler
// ---------------------------------------------------------------------------
impl ApplicationHandler for ShowcaseApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() { return; }
        let attrs = WindowAttributes::default()
            .with_title("Sabitori GPU Showcase — 30 Demos")
            .with_inner_size(winit::dpi::LogicalSize::new(1400.0, 900.0));
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let gpu = sabitori::GpuRenderer::new(window.clone());
        let text = TextRenderer::new(&gpu.device, gpu.surface_config.format, &gpu.globals_bind_group_layout);
        self.window = Some(window);
        self.renderer = Some(gpu);
        self.text_renderer = Some(text);

        // Initialize all demos for card view
        for i in 0..DEMO_COUNT {
            self.init_demo(i, false);
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let (Some(w), Some(r)) = (self.window.as_ref(), self.renderer.as_mut()) {
                    r.resize(size.width, size.height, w.scale_factor());
                    self.win_w = size.width as f32 / r.scale_factor;
                    self.win_h = size.height as f32 / r.scale_factor;
                }
                // Reinit all demos to fit new size
                for i in 0..DEMO_COUNT {
                    let is_modal = self.modal_open == Some(i);
                    self.init_demo(i, is_modal);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(r) = self.renderer.as_ref() {
                    let s = r.scale_factor;
                    self.mouse_x = position.x as f32 / s;
                    self.mouse_y = position.y as f32 / s;
                }
            }
            WindowEvent::MouseInput { state, button: winit::event::MouseButton::Left, .. } => {
                self.mouse_pressed = state == ElementState::Pressed;
                if state == ElementState::Pressed {
                    if let Some(modal_idx) = self.modal_open {
                        let (mx, my, mw, mh) = self.modal_rect();
                        if self.mouse_x < mx || self.mouse_x > mx + mw
                            || self.mouse_y < my || self.mouse_y > my + mh
                        {
                            // Click outside modal → close
                            self.modal_target = 0.0;
                        } else {
                            // Click inside modal — send to demo
                            self.modal_click_x = self.mouse_x;
                            self.modal_click_y = self.mouse_y;

                            // Demo-specific click handling
                            match modal_idx {
                                // Explosion: re-explode from click point
                                2 => {
                                    let cx = self.mouse_x;
                                    let cy = self.mouse_y;
                                    let state = &mut self.demos[modal_idx];
                                    for p in &mut state.particles {
                                        let dx = p.x - cx;
                                        let dy = p.y - cy;
                                        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                                        let force = self.rng.range(200.0, 600.0) / (dist * 0.01 + 1.0);
                                        p.vx = dx / dist * force;
                                        p.vy = dy / dist * force - 100.0;
                                        p.life = 1.0;
                                    }
                                }
                                // Fireworks: launch from click
                                8 => {
                                    let (ax, ay, aw, ah) = self.demo_area_in_modal();
                                    for _ in 0..5 {
                                        self.demos[modal_idx].particles.push(Particle::new_full(
                                            self.mouse_x,
                                            ay + ah,
                                            self.rng.range(-30.0, 30.0),
                                            -self.rng.range(250.0, 450.0),
                                            1.0, 4.0,
                                            self.rng.range(0.0, 360.0),
                                        ));
                                    }
                                }
                                // Ripple Click
                                19 => {
                                    self.demos[modal_idx].particles.push(Particle::new_full(
                                        self.mouse_x, self.mouse_y,
                                        0.0, 0.0, 1.0, 0.0, 200.0,
                                    ));
                                }
                                _ => {}
                            }
                        }
                    } else {
                        // No modal open → check if a card was clicked
                        if let Some(card_idx) = self.hit_card(self.mouse_x, self.mouse_y) {
                            self.modal_open = Some(card_idx);
                            self.modal_target = 1.0;
                            self.modal_anim = 0.0;
                            // Reinit demo for modal size
                            self.init_demo(card_idx, true);
                        }
                    }
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                if self.modal_open.is_none() {
                    let dy = match delta {
                        winit::event::MouseScrollDelta::LineDelta(_, y) => y * SCROLL_SPEED,
                        winit::event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                    };
                    let rows = (DEMO_COUNT + GRID_COLS - 1) / GRID_COLS;
                    let (_, ch) = self.card_size();
                    let max_scroll = (rows as f32 * (ch + CARD_PAD + 28.0) + HEADER_H + CARD_PAD - self.win_h).max(0.0);
                    self.scroll_y = (self.scroll_y - dy).clamp(0.0, max_scroll);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if let Key::Named(winit::keyboard::NamedKey::Escape) = &event.logical_key {
                        if self.modal_open.is_some() {
                            self.modal_target = 0.0;
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.033);
                self.last_frame = now;
                self.global_time += dt;

                // FPS
                self.frame_count += 1;
                if self.fps_timer.elapsed().as_secs_f32() >= 0.5 {
                    self.fps = self.frame_count as f32 / self.fps_timer.elapsed().as_secs_f32();
                    self.frame_count = 0;
                    self.fps_timer = Instant::now();
                }

                // Animate modal
                self.modal_anim += (self.modal_target - self.modal_anim) * MODAL_ANIM_SPEED * dt;
                if self.modal_target == 0.0 && self.modal_anim < 0.01 {
                    if let Some(old_idx) = self.modal_open.take() {
                        self.modal_anim = 0.0;
                        // Reinit demo for card size
                        self.init_demo(old_idx, false);
                    }
                }

                // Update all visible demos
                for i in 0..DEMO_COUNT {
                    let is_modal_demo = self.modal_open == Some(i);
                    // In card mode, only update visible cards
                    if !is_modal_demo {
                        let (_, cy, _, ch) = self.card_rect(i);
                        if cy + ch < -50.0 || cy > self.win_h + 50.0 { continue; }
                    }
                    self.update_demo(i, dt, is_modal_demo);
                }

                // --- Build rects first (no borrow on renderer/text_renderer) ---
                let mut rects = Vec::with_capacity(20000);
                let win_w = self.win_w;
                let win_h = self.win_h;
                let fps = self.fps;
                let modal_open = self.modal_open;
                let modal_anim = self.modal_anim;

                // Background
                rects.push(flat_rect(0.0, 0.0, win_w, win_h, 0.0, c(BG)));

                // Header
                rects.push(flat_rect(0.0, 0.0, win_w, HEADER_H, 0.0, c(BG2)));
                rects.push(flat_rect(0.0, HEADER_H - 1.0, win_w, 1.0, 0.0, c(BORDER)));

                // Pre-compute card layout data
                struct CardLayout {
                    x: f32, y: f32, w: f32, h: f32,
                    visible: bool,
                    gpu_only: bool,
                    name: &'static str,
                    category: &'static str,
                }
                let mut card_layouts: Vec<CardLayout> = Vec::with_capacity(DEMO_COUNT);
                for i in 0..DEMO_COUNT {
                    let (cx, cy, cw, ch) = self.card_rect(i);
                    let visible = cy + ch + 28.0 >= 0.0 && cy <= win_h + 10.0;
                    card_layouts.push(CardLayout {
                        x: cx, y: cy, w: cw, h: ch,
                        visible,
                        gpu_only: self.infos[i].gpu_only,
                        name: self.infos[i].name,
                        category: self.infos[i].category,
                    });
                }

                // Grid of cards (skip if modal is open)
                for i in 0..DEMO_COUNT {
                    if modal_open.is_some() { break; }
                    let cl = &card_layouts[i];
                    if !cl.visible { continue; }

                    // Card background with shadow
                    rects.push(shadow_rect(
                        cl.x, cl.y, cl.w, cl.h,
                        CARD_RADIUS, c(BG2),
                        Color::BLACK.with_alpha(0.3), [0.0, 2.0], [8.0, 0.0],
                    ));

                    // Border
                    let border_col = if modal_open == Some(i) { c(PRIMARY) } else { c(BORDER) };
                    rects.push(bordered_rect(cl.x, cl.y, cl.w, cl.h, CARD_RADIUS, Color::new(0.0, 0.0, 0.0, 0.0), border_col, 1.0));

                    // GPU badge
                    if cl.gpu_only {
                        let badge_w = 32.0;
                        let badge_h = 14.0;
                        rects.push(flat_rect(cl.x + cl.w - badge_w - 4.0, cl.y + 4.0, badge_w, badge_h, 4.0, c(ERROR).with_alpha(0.8)));
                    }

                    // Render demo inside card
                    self.render_demo(i, false, &mut rects);
                }

                // Modal overlay rects
                let mut modal_layout: Option<(f32, f32, f32, f32, bool, &'static str, &'static str)> = None;
                if let Some(modal_idx) = modal_open {
                    let alpha = modal_anim.clamp(0.0, 1.0);
                    if alpha > 0.001 {
                        // Fully opaque background — no transparency
                        rects.push(flat_rect(0.0, 0.0, win_w, win_h, 0.0, c(BG)));

                        let (mmx, mmy, mmw, mmh) = self.modal_rect();
                        let scale = 0.8 + alpha * 0.2;
                        let sw = mmw * scale;
                        let sh = mmh * scale;
                        let sx = mmx + (mmw - sw) * 0.5;
                        let sy = mmy + (mmh - sh) * 0.5;

                        rects.push(shadow_rect(
                            sx, sy, sw, sh, 16.0,
                            c(BG2),
                            Color::BLACK.with_alpha(0.5), [0.0, 4.0], [24.0, 4.0],
                        ));
                        rects.push(bordered_rect(sx, sy, sw, sh, 16.0, Color::new(0.0, 0.0, 0.0, 0.0), c(BORDER), 1.0));

                        let modal_gpu_only = self.infos[modal_idx].gpu_only;
                        let modal_name = self.infos[modal_idx].name;
                        let modal_desc = self.infos[modal_idx].desc;

                        rects.push(flat_rect(sx, sy, sw, 44.0, 0.0, c(BG3).with_alpha(0.5)));

                        if modal_gpu_only {
                            rects.push(flat_rect(sx + 12.0, sy + 12.0, 40.0, 20.0, 6.0, c(ERROR)));
                        }

                        // Close button
                        let close_x = sx + sw - 36.0;
                        let close_y = sy + 8.0;
                        rects.push(flat_rect(close_x, close_y, 28.0, 28.0, 6.0, c(BG3)));

                        // Render demo inside modal
                        self.render_demo(modal_idx, true, &mut rects);

                        modal_layout = Some((sx, sy, sw, sh, modal_gpu_only, modal_name, modal_desc));
                    }
                }

                // --- Now borrow text_renderer and renderer for text + submit ---
                let (Some(renderer), Some(tr)) = (self.renderer.as_mut(), self.text_renderer.as_mut()) else { return; };

                let mut glyphs = Vec::new();

                // Header text
                glyphs.extend(tr.prepare_text(
                    "Sabitori GPU Showcase",
                    16.0, 10.0, 18.0, c(TEXT_COL), None,
                ));
                let fps_text = format!("{:.0} FPS", fps);
                glyphs.extend(tr.prepare_text(
                    &fps_text,
                    win_w - 90.0, 10.0, 14.0, c(SUCCESS), None,
                ));
                glyphs.extend(tr.prepare_text(
                    "Click card to expand  |  Escape to close  |  Scroll to browse",
                    16.0, 32.0, 11.0, c(TEXT2), None,
                ));

                // Card labels and badges
                for i in 0..DEMO_COUNT {
                    if modal_open.is_some() { break; }
                    let cl = &card_layouts[i];
                    if !cl.visible { continue; }

                    if cl.gpu_only {
                        let badge_w = 32.0;
                        glyphs.extend(tr.prepare_text(
                            "GPU", cl.x + cl.w - badge_w, cl.y + 4.0, 9.0, Color::WHITE, None,
                        ));
                    }

                    let label_y = cl.y + cl.h + 4.0;
                    glyphs.extend(tr.prepare_text(
                        cl.name, cl.x + 4.0, label_y, 11.0, c(TEXT_COL), Some(cl.w - 8.0),
                    ));
                    glyphs.extend(tr.prepare_text(
                        cl.category, cl.x + 4.0, label_y + 13.0, 9.0, c(TEXT2), Some(cl.w - 8.0),
                    ));
                }

                // Modal text
                if let Some((sx, sy, sw, _sh, is_gpu, name, desc)) = modal_layout {
                    if is_gpu {
                        glyphs.extend(tr.prepare_text("GPU", sx + 17.0, sy + 13.0, 12.0, Color::WHITE, None));
                    }
                    let title_x = if is_gpu { sx + 60.0 } else { sx + 12.0 };
                    glyphs.extend(tr.prepare_text(
                        name, title_x, sy + 8.0, 16.0, c(TEXT_COL), None,
                    ));
                    glyphs.extend(tr.prepare_text(
                        desc, title_x, sy + 27.0, 11.0, c(TEXT2), Some(sw - 80.0),
                    ));
                    let close_x = sx + sw - 36.0;
                    let close_y = sy + 8.0;
                    glyphs.extend(tr.prepare_text("X", close_x + 9.0, close_y + 6.0, 14.0, c(TEXT_COL), None));
                }

                let device = renderer.device.clone();
                let queue = renderer.queue.clone();
                let _ = renderer.render_with(&rects, |pass, globals_bg| {
                    tr.render_glyphs(&device, &queue, &glyphs, pass, globals_bg);
                });
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = self.window.as_ref() { w.request_redraw(); }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")))
        .init();
    let event_loop = EventLoop::new().unwrap();
    let mut app = ShowcaseApp::new();
    event_loop.run_app(&mut app).unwrap();
}
