use std::sync::Arc;
use std::time::Instant;

use sabitori::{
    Animated, AnimationMode, Color, GlyphInstance, RectInstance, Spring, TextRenderer,
};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

struct AnimCard {
    x: f32,
    y: f32,
    color: Color,
    label: &'static str,
    scale: Animated<f32>,
    glow: Animated<f32>,
    hovered: bool,
}

struct AnimApp {
    window: Option<Arc<Window>>,
    renderer: Option<sabitori::GpuRenderer>,
    text_renderer: Option<TextRenderer>,
    last_frame: Instant,
    cards: Vec<AnimCard>,
    mouse_x: f32,
    mouse_y: f32,
    // Floating panel
    panel_y: Animated<f32>,
    panel_visible: bool,
    // FPS counter
    frame_count: u32,
    fps_timer: Instant,
    current_fps: f32,
}

impl AnimApp {
    fn new() -> Self {
        let colors = [
            ("#6c63ff", "Snappy Spring"),
            ("#e84393", "Bouncy Spring"),
            ("#00cec9", "Gentle Spring"),
            ("#fdcb6e", "Critical Damp"),
        ];

        let springs = [
            Spring::snappy(),
            Spring::bouncy(),
            Spring::gentle(),
            Spring::default(),
        ];

        let cards: Vec<AnimCard> = colors
            .iter()
            .zip(springs.iter())
            .enumerate()
            .map(|(i, ((hex, label), spring))| {
                let x = 60.0 + i as f32 * 260.0;
                AnimCard {
                    x,
                    y: 120.0,
                    color: Color::from_hex(hex),
                    label,
                    scale: Animated::new(1.0).with_spring(*spring),
                    glow: Animated::new(0.0).with_spring(*spring),
                    hovered: false,
                }
            })
            .collect();

        Self {
            window: None,
            renderer: None,
            text_renderer: None,
            last_frame: Instant::now(),
            cards,
            mouse_x: 0.0,
            mouse_y: 0.0,
            panel_y: Animated::new(600.0).with_spring(Spring::bouncy()),
            panel_visible: false,
            frame_count: 0,
            fps_timer: Instant::now(),
            current_fps: 0.0,
        }
    }
}

impl ApplicationHandler for AnimApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = WindowAttributes::default()
            .with_title("Sabitori - Spring Animations")
            .with_inner_size(winit::dpi::LogicalSize::new(1100.0, 700.0));

        let window = Arc::new(event_loop.create_window(attrs).unwrap());
        let gpu = sabitori::GpuRenderer::new(window.clone());
        let text_renderer = TextRenderer::new(
            &gpu.device,
            gpu.surface_config.format,
            &gpu.globals_bind_group_layout,
        );
        self.window = Some(window);
        self.renderer = Some(gpu);
        self.text_renderer = Some(text_renderer);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let (Some(w), Some(r)) = (self.window.as_ref(), self.renderer.as_mut()) {
                    r.resize(size.width, size.height, w.scale_factor());
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                if let Some(r) = self.renderer.as_ref() {
                    let s = r.scale_factor;
                    self.mouse_x = position.x as f32 / s;
                    self.mouse_y = position.y as f32 / s;
                }

                // Hit test cards
                for card in &mut self.cards {
                    let w = 220.0;
                    let h = 280.0;
                    let in_card = self.mouse_x >= card.x
                        && self.mouse_x <= card.x + w
                        && self.mouse_y >= card.y
                        && self.mouse_y <= card.y + h;

                    if in_card && !card.hovered {
                        card.hovered = true;
                        card.scale.set_target(1.08);
                        card.glow.set_target(1.0);
                    } else if !in_card && card.hovered {
                        card.hovered = false;
                        card.scale.set_target(1.0);
                        card.glow.set_target(0.0);
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                // Toggle panel
                self.panel_visible = !self.panel_visible;
                if self.panel_visible {
                    self.panel_y.set_target(400.0);
                } else {
                    self.panel_y.set_target(700.0);
                }

                // Press animation on hovered card
                for card in &mut self.cards {
                    if card.hovered {
                        card.scale.set_target(0.95);
                    }
                }
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: winit::event::MouseButton::Left,
                ..
            } => {
                for card in &mut self.cards {
                    if card.hovered {
                        card.scale.set_target(1.08);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.05);
                self.last_frame = now;

                // Tick animations
                for card in &mut self.cards {
                    card.scale.tick(dt);
                    card.glow.tick(dt);
                }
                self.panel_y.tick(dt);

                let (Some(renderer), Some(text_renderer)) =
                    (self.renderer.as_mut(), self.text_renderer.as_mut())
                else {
                    return;
                };

                let scale = renderer.scale_factor;
                let w = renderer.surface_config.width as f32 / scale;

                let bg = Color::from_hex("#1a1a2e");
                let surface = Color::from_hex("#22223a");
                let border = Color::from_hex("#3a3a55");
                let text_color = Color::from_hex("#e8e8f0");
                let text_sub = Color::from_hex("#9090a8");

                let mut rects = vec![
                    // Background
                    RectInstance {
                        rect: [0.0, 0.0, 2000.0, 2000.0],
                        fill_color: bg.to_array(),
                        corner_radii: [0.0; 4],
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                        shadow_color: [0.0; 4],
                        shadow_offset: [0.0; 2],
                        shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                    },
                ];

                // FPS counter
                self.frame_count += 1;
                let elapsed = self.fps_timer.elapsed().as_secs_f32();
                if elapsed >= 0.5 {
                    self.current_fps = self.frame_count as f32 / elapsed;
                    self.frame_count = 0;
                    self.fps_timer = Instant::now();
                }

                let mut glyphs: Vec<GlyphInstance> = Vec::new();

                // Title with FPS
                let title = format!(
                    "Spring Animation Demo — {:.0} FPS",
                    self.current_fps
                );
                glyphs.extend(text_renderer.prepare_text(
                    &title,
                    60.0, 50.0, 18.0, text_color, Some(w - 120.0),
                ));
                glyphs.extend(text_renderer.prepare_text(
                    "ホバーで各カードが異なるスプリングで反応 / クリックでパネル開閉",
                    60.0, 80.0, 14.0, text_sub, Some(w - 120.0),
                ));

                // Cards with spring-animated scale
                let card_w = 220.0;
                let card_h = 280.0;

                for card in &self.cards {
                    let s = card.scale.value();
                    let glow = card.glow.value();

                    // Calculate scaled bounds (centered scaling)
                    let cx = card.x + card_w / 2.0;
                    let cy = card.y + card_h / 2.0;
                    let sw = card_w * s;
                    let sh = card_h * s;
                    let sx = cx - sw / 2.0;
                    let sy = cy - sh / 2.0;

                    // Shadow (grows with hover)
                    let shadow_blur = 16.0 + glow * 16.0;
                    let shadow_spread = 2.0 + glow * 6.0;
                    let shadow_color = Color::new(0.0, 0.0, 0.0, 0.3 + glow * 0.2);

                    // Glow border
                    let glow_border = card.color.with_alpha(0.3 + glow * 0.5);

                    rects.push(RectInstance {
                        rect: [sx, sy, sw, sh],
                        fill_color: surface.to_array(),
                        corner_radii: [12.0; 4],
                        border_color: glow_border.to_array(),
                        border_width: 1.0 + glow,
                        gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                        shadow_color: shadow_color.to_array(),
                        shadow_offset: [0.0, 4.0 + glow * 4.0],
                        shadow_params: [shadow_blur, shadow_spread],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                    });

                    // Accent bar
                    rects.push(RectInstance {
                        rect: [sx, sy, sw, 4.0],
                        fill_color: card.color.to_array(),
                        corner_radii: [12.0, 12.0, 0.0, 0.0],
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                        shadow_color: [0.0; 4],
                        shadow_offset: [0.0; 2],
                        shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                    });

                    // Circle
                    let circle_s = 44.0 * s;
                    rects.push(RectInstance {
                        rect: [sx + 20.0, sy + 24.0, circle_s, circle_s],
                        fill_color: card.color.with_alpha(0.15 + glow * 0.15).to_array(),
                        corner_radii: [circle_s / 2.0; 4],
                        border_color: card.color.to_array(),
                        border_width: 2.0,
                        gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                        shadow_color: [0.0; 4],
                        shadow_offset: [0.0; 2],
                        shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                    });

                    // Label
                    glyphs.extend(text_renderer.prepare_text(
                        card.label, sx + 20.0, sy + 84.0, 15.0, text_color, Some(sw - 40.0),
                    ));

                    // Spring type description
                    let desc = if card.label.contains("Snappy") {
                        "素早く収束、わずかなオーバーシュート"
                    } else if card.label.contains("Bouncy") {
                        "弾むようなバウンス効果"
                    } else if card.label.contains("Gentle") {
                        "ゆっくり滑らかに収束"
                    } else {
                        "臨界減衰 — 最速で収束"
                    };
                    glyphs.extend(text_renderer.prepare_text(
                        desc, sx + 20.0, sy + 110.0, 12.0, text_sub, Some(sw - 40.0),
                    ));

                    // Scale indicator
                    let scale_text = format!("scale: {:.3}", s);
                    glyphs.extend(text_renderer.prepare_text(
                        &scale_text, sx + 20.0, sy + sh - 40.0, 13.0,
                        card.color, Some(sw - 40.0),
                    ));
                }

                // Floating panel (animated Y position)
                let panel_y = self.panel_y.value();
                let panel_w = w - 120.0;
                let panel_h = 200.0;
                let panel_x = 60.0;

                if panel_y < 700.0 {
                    rects.push(RectInstance {
                        rect: [panel_x, panel_y, panel_w, panel_h],
                        fill_color: Color::from_hex("#22223a").with_alpha(0.95).to_array(),
                        corner_radii: [16.0; 4],
                        border_color: Color::from_hex("#6c63ff60").to_array(),
                        border_width: 1.0,
                        gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                        shadow_color: Color::from_hex("#00000080").to_array(),
                        shadow_offset: [0.0, -4.0],
                        shadow_params: [32.0, 8.0],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                    });

                    glyphs.extend(text_renderer.prepare_text(
                        "スプリングアニメーションパネル",
                        panel_x + 24.0, panel_y + 24.0, 18.0,
                        text_color, Some(panel_w - 48.0),
                    ));
                    glyphs.extend(text_renderer.prepare_text(
                        "このパネルはBouncy Springで出現します。クリックで開閉。",
                        panel_x + 24.0, panel_y + 56.0, 14.0,
                        text_sub, Some(panel_w - 48.0),
                    ));
                    glyphs.extend(text_renderer.prepare_text(
                        "sabitori-anim: Spring / Easing / Animated<T> で任意の値をアニメーション可能",
                        panel_x + 24.0, panel_y + 84.0, 14.0,
                        text_sub, Some(panel_w - 48.0),
                    ));
                }

                // Render
                let device = renderer.device.clone();
                let queue = renderer.queue.clone();
                let _ = renderer.render_with(&rects, |pass, globals_bg| {
                    text_renderer.render_glyphs(&device, &queue, &glyphs, pass, globals_bg);
                });
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(w) = self.window.as_ref() {
            w.request_redraw();
        }
    }
}

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    let event_loop = EventLoop::new().unwrap();
    let mut app = AnimApp::new();
    event_loop.run_app(&mut app).unwrap();
}
