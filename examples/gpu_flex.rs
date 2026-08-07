/// GPU Flex — Sabitori performance showcase
/// Things that would be impossible or janky in CSS/DOM:
/// - 10,000 particles at 120fps
/// - Real-time gravity + collision physics
/// - Mouse-repulsion force field
/// - Every element is a GPU-instanced SDF rounded rect

use std::sync::Arc;
use std::time::Instant;

use sabitori::{Color, GlyphInstance, RectInstance, TextRenderer};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

const PARTICLE_COUNT: usize = 10_000;
const GRAVITY: f32 = 400.0;
const MOUSE_FORCE: f32 = 80_000.0;
const MOUSE_RADIUS: f32 = 150.0;
const DAMPING: f32 = 0.98;
const BOUNCE: f32 = 0.7;
const PARTICLE_SIZE: f32 = 4.0;

struct Particle {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    radius: f32,
    hue: f32,
}

struct GpuFlexApp {
    window: Option<Arc<Window>>,
    renderer: Option<sabitori::GpuRenderer>,
    text_renderer: Option<TextRenderer>,
    last_frame: Instant,
    particles: Vec<Particle>,
    mouse_x: f32,
    mouse_y: f32,
    mouse_pressed: bool,
    win_w: f32,
    win_h: f32,
    frame_count: u32,
    fps_timer: Instant,
    fps: f32,
    mode: usize, // 0=gravity+repel, 1=attract, 2=explode, 3=wave
    time: f32,
}

impl GpuFlexApp {
    fn new() -> Self {
        // Initialize particles in a grid
        let mut particles = Vec::with_capacity(PARTICLE_COUNT);
        let cols = (PARTICLE_COUNT as f32).sqrt() as usize;
        for i in 0..PARTICLE_COUNT {
            let col = i % cols;
            let row = i / cols;
            let x = 100.0 + (col as f32 / cols as f32) * 900.0;
            let y = 50.0 + (row as f32 / (PARTICLE_COUNT / cols) as f32) * 600.0;
            let hue = (i as f32 / PARTICLE_COUNT as f32) * 360.0;
            particles.push(Particle {
                x, y,
                vx: 0.0, vy: 0.0,
                radius: PARTICLE_SIZE * 0.5 + (i % 3) as f32 * 0.5,
                hue,
            });
        }

        Self {
            window: None,
            renderer: None,
            text_renderer: None,
            last_frame: Instant::now(),
            particles,
            mouse_x: 600.0,
            mouse_y: 400.0,
            mouse_pressed: false,
            win_w: 1200.0,
            win_h: 800.0,
            frame_count: 0,
            fps_timer: Instant::now(),
            fps: 0.0,
            mode: 0,
            time: 0.0,
        }
    }

    fn update_particles(&mut self, dt: f32) {
        let dt = dt.min(0.016); // cap at ~60fps physics step
        let w = self.win_w;
        let h = self.win_h;
        let mx = self.mouse_x;
        let my = self.mouse_y;
        let pressed = self.mouse_pressed;
        let mode = self.mode;
        let time = self.time;

        for p in &mut self.particles {
            match mode {
                0 => {
                    // Gravity + mouse repulsion
                    p.vy += GRAVITY * dt;

                    if pressed {
                        let dx = p.x - mx;
                        let dy = p.y - my;
                        let dist_sq = dx * dx + dy * dy + 1.0;
                        let dist = dist_sq.sqrt();
                        if dist < MOUSE_RADIUS {
                            let force = MOUSE_FORCE / dist_sq;
                            p.vx += dx / dist * force * dt;
                            p.vy += dy / dist * force * dt;
                        }
                    }
                }
                1 => {
                    // Attract to mouse
                    let dx = mx - p.x;
                    let dy = my - p.y;
                    let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                    let force = 200.0;
                    p.vx += dx / dist * force * dt;
                    p.vy += dy / dist * force * dt;
                    // Orbital tangent
                    p.vx += -dy / dist * 50.0 * dt;
                    p.vy += dx / dist * 50.0 * dt;
                }
                2 => {
                    // Explode from center, then gravity
                    p.vy += GRAVITY * 0.5 * dt;
                    // Wind
                    p.vx += (time * 0.5).sin() * 30.0 * dt;
                }
                3 => {
                    // Wave field — particles follow a sine wave pattern
                    let target_y = h * 0.5 + ((p.x * 0.02 + time * 2.0).sin() * 100.0)
                        + ((p.x * 0.05 + time * 3.0).sin() * 50.0);
                    let target_x = p.x; // keep x roughly stable
                    p.vx += (target_x - p.x) * 2.0 * dt;
                    p.vy += (target_y - p.y) * 5.0 * dt;

                    if pressed {
                        let dx = p.x - mx;
                        let dy = p.y - my;
                        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                        if dist < 200.0 {
                            p.vy += -300.0 * dt * (1.0 - dist / 200.0);
                        }
                    }
                }
                _ => {}
            }

            // Apply velocity
            p.vx *= DAMPING;
            p.vy *= DAMPING;
            p.x += p.vx * dt;
            p.y += p.vy * dt;

            // Bounce off walls
            if p.x < p.radius {
                p.x = p.radius;
                p.vx = p.vx.abs() * BOUNCE;
            }
            if p.x > w - p.radius {
                p.x = w - p.radius;
                p.vx = -p.vx.abs() * BOUNCE;
            }
            if p.y < p.radius {
                p.y = p.radius;
                p.vy = p.vy.abs() * BOUNCE;
            }
            if p.y > h - p.radius {
                p.y = h - p.radius;
                p.vy = -p.vy.abs() * BOUNCE;
            }
        }
    }
}

fn hue_to_color(hue: f32, speed: f32, alpha: f32) -> Color {
    let speed_factor = (speed / 500.0).min(1.0);
    // Shift hue based on velocity for visual interest
    let h = (hue + speed_factor * 60.0) % 360.0;
    let s = 0.7 + speed_factor * 0.3;
    let v = 0.6 + speed_factor * 0.4;
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let m = v - c;
    let (r, g, b) = if h < 60.0 { (c, x, 0.0) }
        else if h < 120.0 { (x, c, 0.0) }
        else if h < 180.0 { (0.0, c, x) }
        else if h < 240.0 { (0.0, x, c) }
        else if h < 300.0 { (x, 0.0, c) }
        else { (c, 0.0, x) };
    Color::new(r + m, g + m, b + m, alpha)
}

impl ApplicationHandler for GpuFlexApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() { return; }
        let attrs = WindowAttributes::default()
            .with_title("Sabitori GPU Flex — 10,000 Particles")
            .with_inner_size(winit::dpi::LogicalSize::new(1200.0, 800.0));
        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let gpu = sabitori::GpuRenderer::new(window.clone());
        let text = TextRenderer::new(&gpu.device, gpu.surface_config.format, &gpu.globals_bind_group_layout);
        self.window = Some(window);
        self.renderer = Some(gpu);
        self.text_renderer = Some(text);
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
                // On click in mode 2, re-explode
                if state == ElementState::Pressed && self.mode == 2 {
                    for p in &mut self.particles {
                        let dx = p.x - self.mouse_x;
                        let dy = p.y - self.mouse_y;
                        let dist = (dx * dx + dy * dy).sqrt().max(1.0);
                        let force = 800.0 / (dist * 0.1 + 1.0);
                        p.vx = dx / dist * force;
                        p.vy = dy / dist * force - 200.0;
                    }
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed {
                    if let winit::keyboard::Key::Character(ch) = &event.logical_key {
                        match ch.as_str() {
                            "1" => self.mode = 0,
                            "2" => self.mode = 1,
                            "3" => {
                                self.mode = 2;
                                // Initial explosion from center
                                let cx = self.win_w / 2.0;
                                let cy = self.win_h / 2.0;
                                for p in &mut self.particles {
                                    let angle = p.hue / 360.0 * std::f32::consts::TAU;
                                    let speed = 200.0 + (p.radius - 2.0) * 200.0;
                                    p.x = cx;
                                    p.y = cy;
                                    p.vx = angle.cos() * speed;
                                    p.vy = angle.sin() * speed - 300.0;
                                }
                            }
                            "4" => self.mode = 3,
                            _ => {}
                        }
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.05);
                self.last_frame = now;
                self.time += dt;

                // Physics
                self.update_particles(dt);

                // FPS
                self.frame_count += 1;
                if self.fps_timer.elapsed().as_secs_f32() >= 0.5 {
                    self.fps = self.frame_count as f32 / self.fps_timer.elapsed().as_secs_f32();
                    self.frame_count = 0;
                    self.fps_timer = Instant::now();
                }

                let (Some(renderer), Some(tr)) = (self.renderer.as_mut(), self.text_renderer.as_mut()) else { return; };

                let bg = Color::from_hex("#0a0a14");

                // Build rect instances - 1 background + 10,000 particles = 10,001 draw calls in ONE instanced batch
                let mut rects = Vec::with_capacity(PARTICLE_COUNT + 10);

                // Background
                rects.push(RectInstance {
                    rect: [0.0, 0.0, self.win_w, self.win_h],
                    fill_color: bg.to_array(),
                    corner_radii: [0.0; 4],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0,
                    shadow_color: [0.0; 4],
                    shadow_offset: [0.0; 2],
                    shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                        clip_rect: [0.0; 4],
                });

                // Mouse cursor indicator
                if self.mouse_pressed {
                    let ring_alpha = 0.15;
                    rects.push(RectInstance {
                        rect: [self.mouse_x - MOUSE_RADIUS, self.mouse_y - MOUSE_RADIUS,
                               MOUSE_RADIUS * 2.0, MOUSE_RADIUS * 2.0],
                        fill_color: Color::new(1.0, 1.0, 1.0, 0.03).to_array(),
                        corner_radii: [MOUSE_RADIUS; 4],
                        border_color: Color::new(1.0, 1.0, 1.0, ring_alpha).to_array(),
                        border_width: 1.0,
                        gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0,
                        shadow_color: [0.0; 4],
                        shadow_offset: [0.0; 2],
                        shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                        clip_rect: [0.0; 4],
                    });
                }

                // All 10,000 particles as instanced rects
                for p in &self.particles {
                    let speed = (p.vx * p.vx + p.vy * p.vy).sqrt();
                    let col = hue_to_color(p.hue, speed, 0.85);
                    let r = p.radius;
                    rects.push(RectInstance {
                        rect: [p.x - r, p.y - r, r * 2.0, r * 2.0],
                        fill_color: col.to_array(),
                        corner_radii: [r; 4], // circle
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0,
                        shadow_color: if speed > 100.0 {
                            col.with_alpha(0.3).to_array()
                        } else {
                            [0.0; 4]
                        },
                        shadow_offset: [0.0; 2],
                        shadow_params: if speed > 100.0 {
                            [(speed / 100.0).min(8.0), 0.0]
                        } else {
                            [0.0; 2]
                        },
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                        clip_rect: [0.0; 4],
                    });
                }

                // HUD text
                let mut glyphs = Vec::new();
                let hud = format!(
                    "{:.0} FPS | {} particles | Mode {}: {}",
                    self.fps, PARTICLE_COUNT,
                    self.mode + 1,
                    match self.mode {
                        0 => "Gravity + Mouse Repel (click & drag)",
                        1 => "Orbit Attract",
                        2 => "Explosion (click to re-explode)",
                        3 => "Wave Field (click to disturb)",
                        _ => "",
                    }
                );
                glyphs.extend(tr.prepare_text(&hud, 16.0, 14.0, 14.0, Color::from_hex("#c0caf5"), None));
                glyphs.extend(tr.prepare_text(
                    "キー 1-4 でモード切替 | マウスクリック&ドラッグで操作",
                    16.0, 36.0, 12.0, Color::from_hex("#9aa5ce"), None,
                ));

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
    let mut app = GpuFlexApp::new();
    event_loop.run_app(&mut app).unwrap();
}
