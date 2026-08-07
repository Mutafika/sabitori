/// Sabitori GPU Effects Demo
/// 4 practical effects that CSS can't match:
/// 1. Mouse-following spotlight glow (Stripe-style)
/// 2. Magnetic snap buttons
/// 3. Gravity dropdown
/// 4. Fluid menu indicator

use std::sync::Arc;
use std::time::Instant;

use sabitori::{Color, GlyphInstance, RectInstance, TextRenderer};
use winit::application::ApplicationHandler;
use winit::event::{ElementState, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

fn c(hex: &str) -> Color { Color::from_hex(hex) }

// ---------------------------------------------------------------------------
// Effect 1: Spotlight Glow — cursor light illuminates cards
// ---------------------------------------------------------------------------
struct SpotlightGlow {
    cards: Vec<SpotlightCard>,
}

struct SpotlightCard {
    x: f32, y: f32, w: f32, h: f32,
    label: &'static str,
    desc: &'static str,
}

impl SpotlightGlow {
    fn new() -> Self {
        let gap = 20.0;
        let cw = 240.0;
        let ch = 140.0;
        let start_x = 40.0;
        let start_y = 80.0;
        Self {
            cards: vec![
                SpotlightCard { x: start_x, y: start_y, w: cw, h: ch, label: "Analytics", desc: "リアルタイムダッシュボード" },
                SpotlightCard { x: start_x + cw + gap, y: start_y, w: cw, h: ch, label: "Deployment", desc: "ワンクリックデプロイ" },
                SpotlightCard { x: start_x + (cw + gap) * 2.0, y: start_y, w: cw, h: ch, label: "Monitoring", desc: "24/7サーバー監視" },
            ],
        }
    }

    fn render(
        &self, mx: f32, my: f32, _time: f32,
        rects: &mut Vec<RectInstance>, glyphs: &mut Vec<GlyphInstance>, tr: &mut TextRenderer,
        area_x: f32, area_y: f32,
    ) {
        let ox = area_x;
        let oy = area_y;

        for card in &self.cards {
            let cx = card.x + ox;
            let cy = card.y + oy;

            // Distance from mouse to card center
            let card_cx = cx + card.w / 2.0;
            let card_cy = cy + card.h / 2.0;
            let dx = mx - card_cx;
            let dy = my - card_cy;
            let dist = (dx * dx + dy * dy).sqrt();

            // Glow intensity based on proximity
            let intensity = (1.0 - (dist / 400.0)).clamp(0.0, 1.0);
            let intensity = intensity * intensity; // quadratic falloff

            // Glow position relative to card (where the light hits)
            let glow_x = ((mx - cx) / card.w).clamp(0.0, 1.0);
            let glow_y = ((my - cy) / card.h).clamp(0.0, 1.0);

            // Card base
            let border_color = Color::new(
                0.48 * intensity + 0.25 * (1.0 - intensity),
                0.63 * intensity + 0.28 * (1.0 - intensity),
                0.97 * intensity + 0.35 * (1.0 - intensity),
                0.3 + intensity * 0.7,
            );

            rects.push(RectInstance {
                rect: [cx, cy, card.w, card.h],
                fill_color: Color::from_hex("#1e1e32").to_array(),
                corner_radii: [12.0; 4],
                border_color: border_color.to_array(),
                border_width: 1.0 + intensity,
                gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                shadow_color: Color::new(0.48, 0.63, 0.97, intensity * 0.3).to_array(),
                shadow_offset: [0.0; 2],
                shadow_params: [intensity * 30.0, intensity * 8.0],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
            });

            // Glow spot (simulated radial gradient via overlapping rects)
            if intensity > 0.05 {
                let spot_size = 60.0;
                let spot_x = cx + glow_x * card.w - spot_size / 2.0;
                let spot_y = cy + glow_y * card.h - spot_size / 2.0;
                // Clamp spot within card
                let spot_x = spot_x.clamp(cx, cx + card.w - spot_size);
                let spot_y = spot_y.clamp(cy, cy + card.h - spot_size);

                rects.push(RectInstance {
                    rect: [spot_x, spot_y, spot_size, spot_size],
                    fill_color: Color::new(0.48, 0.63, 0.97, intensity * 0.04).to_array(),
                    corner_radii: [spot_size / 2.0; 4],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                    shadow_color: [0.0; 4],
                    shadow_offset: [0.0; 2],
                    shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                });

                // Inner brighter spot
                let inner = spot_size * 0.5;
                rects.push(RectInstance {
                    rect: [spot_x + (spot_size - inner) / 2.0, spot_y + (spot_size - inner) / 2.0, inner, inner],
                    fill_color: Color::new(0.48, 0.63, 0.97, intensity * 0.03).to_array(),
                    corner_radii: [inner / 2.0; 4],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                    shadow_color: [0.0; 4],
                    shadow_offset: [0.0; 2],
                    shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                });
            }

            // Text
            let text_alpha = 0.6 + intensity * 0.4;
            glyphs.extend(tr.prepare_text(
                card.label, cx + 20.0, cy + 24.0, 18.0,
                Color::new(0.75, 0.79, 0.96, text_alpha), None,
            ));
            glyphs.extend(tr.prepare_text(
                card.desc, cx + 20.0, cy + 52.0, 13.0,
                Color::new(0.6, 0.65, 0.81, text_alpha * 0.7), None,
            ));

            // Fake icon glow
            let icon_x = cx + card.w - 50.0;
            let icon_y = cy + card.h - 50.0;
            rects.push(RectInstance {
                rect: [icon_x, icon_y, 30.0, 30.0],
                fill_color: Color::new(0.48, 0.63, 0.97, 0.1 + intensity * 0.2).to_array(),
                corner_radii: [6.0; 4],
                border_color: Color::new(0.48, 0.63, 0.97, 0.2 + intensity * 0.5).to_array(),
                border_width: 1.0,
                gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                shadow_color: [0.0; 4],
                shadow_offset: [0.0; 2],
                shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
            });
            glyphs.extend(tr.prepare_text("→", icon_x + 7.0, icon_y + 6.0, 14.0,
                Color::new(0.48, 0.63, 0.97, 0.5 + intensity * 0.5), None));
        }
    }
}

// ---------------------------------------------------------------------------
// Effect 4: Magnetic Snap Buttons
// ---------------------------------------------------------------------------
struct MagneticButtons {
    buttons: Vec<MagButton>,
}

struct MagButton {
    base_x: f32, base_y: f32,
    w: f32, h: f32,
    offset_x: f32, offset_y: f32,
    label: &'static str,
    color: Color,
}

impl MagneticButtons {
    fn new() -> Self {
        Self {
            buttons: vec![
                MagButton {
                    base_x: 80.0, base_y: 120.0, w: 160.0, h: 48.0,
                    offset_x: 0.0, offset_y: 0.0,
                    label: "Get Started",
                    color: c("#7aa2f7"),
                },
                MagButton {
                    base_x: 280.0, base_y: 120.0, w: 160.0, h: 48.0,
                    offset_x: 0.0, offset_y: 0.0,
                    label: "Learn More",
                    color: c("#bb9af7"),
                },
                MagButton {
                    base_x: 480.0, base_y: 120.0, w: 160.0, h: 48.0,
                    offset_x: 0.0, offset_y: 0.0,
                    label: "Contact",
                    color: c("#9ece6a"),
                },
            ],
        }
    }

    fn update(&mut self, mx: f32, my: f32, area_x: f32, area_y: f32) {
        let mag_radius = 120.0;
        let max_offset = 12.0;
        let spring = 0.15; // smooth follow

        for btn in &mut self.buttons {
            let cx = btn.base_x + area_x + btn.w / 2.0;
            let cy = btn.base_y + area_y + btn.h / 2.0;
            let dx = mx - cx;
            let dy = my - cy;
            let dist = (dx * dx + dy * dy).sqrt();

            if dist < mag_radius {
                let strength = (1.0 - dist / mag_radius).powi(2);
                let target_x = dx * strength * max_offset / mag_radius.max(1.0);
                let target_y = dy * strength * max_offset / mag_radius.max(1.0);
                btn.offset_x += (target_x - btn.offset_x) * spring;
                btn.offset_y += (target_y - btn.offset_y) * spring;
            } else {
                btn.offset_x *= 1.0 - spring;
                btn.offset_y *= 1.0 - spring;
            }
        }
    }

    fn render(
        &self, mx: f32, my: f32,
        rects: &mut Vec<RectInstance>, glyphs: &mut Vec<GlyphInstance>, tr: &mut TextRenderer,
        area_x: f32, area_y: f32,
    ) {
        for btn in &self.buttons {
            let bx = btn.base_x + area_x + btn.offset_x;
            let by = btn.base_y + area_y + btn.offset_y;
            let offset_mag = (btn.offset_x.powi(2) + btn.offset_y.powi(2)).sqrt();
            let pull = (offset_mag / 12.0).min(1.0);

            rects.push(RectInstance {
                rect: [bx, by, btn.w, btn.h],
                fill_color: btn.color.with_alpha(0.25 + pull * 0.15).to_array(),
                corner_radii: [10.0; 4],
                border_color: btn.color.with_alpha(0.5 + pull * 0.5).to_array(),
                border_width: 1.0 + pull,
                gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                shadow_color: btn.color.with_alpha(pull * 0.3).to_array(),
                shadow_offset: [btn.offset_x * 0.5, btn.offset_y * 0.5 + 2.0],
                shadow_params: [8.0 + pull * 16.0, pull * 4.0],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
            });

            let text_w = btn.label.len() as f32 * 4.5;
            glyphs.extend(tr.prepare_text(
                btn.label,
                bx + (btn.w - text_w) / 2.0,
                by + 15.0, 14.0,
                Color::WHITE.with_alpha(0.8 + pull * 0.2),
                None,
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Effect 5: Gravity Dropdown
// ---------------------------------------------------------------------------
struct GravityDropdown {
    open: bool,
    items: Vec<GravItem>,
    trigger_y: f32,
}

struct GravItem {
    label: &'static str,
    y: f32,      // current y position
    vy: f32,     // velocity
    target_y: f32,
    settled: bool,
}

impl GravityDropdown {
    fn new() -> Self {
        let items = ["Dashboard", "Settings", "Profile", "Billing", "Logout"];
        Self {
            open: false,
            items: items.iter().enumerate().map(|(i, &label)| {
                GravItem {
                    label,
                    y: -200.0, // start above
                    vy: 0.0,
                    target_y: 50.0 + i as f32 * 40.0,
                    settled: false,
                }
            }).collect(),
            trigger_y: 0.0,
        }
    }

    fn toggle(&mut self) {
        self.open = !self.open;
        if self.open {
            for (i, item) in self.items.iter_mut().enumerate() {
                item.y = -40.0 - i as f32 * 20.0; // stagger start positions
                item.vy = 0.0;
                item.settled = false;
            }
        }
    }

    fn update(&mut self, dt: f32) {
        if !self.open { return; }
        let gravity = 1800.0;
        let damping = 0.65;

        for item in &mut self.items {
            if item.settled { continue; }

            item.vy += gravity * dt;
            item.y += item.vy * dt;

            // Bounce off target position
            if item.y > item.target_y {
                item.y = item.target_y;
                item.vy = -item.vy * damping;
                if item.vy.abs() < 5.0 {
                    item.vy = 0.0;
                    item.y = item.target_y;
                    item.settled = true;
                }
            }
        }
    }

    fn render(
        &self, mx: f32, my: f32,
        rects: &mut Vec<RectInstance>, glyphs: &mut Vec<GlyphInstance>, tr: &mut TextRenderer,
        area_x: f32, area_y: f32,
    ) {
        let bx = area_x + 60.0;
        let by = area_y + 30.0;
        let bw = 200.0;

        // Trigger button
        let label = if self.open { "Close Menu ▲" } else { "Open Menu ▼" };
        rects.push(RectInstance {
            rect: [bx, by, bw, 40.0],
            fill_color: c("#7aa2f7").with_alpha(0.2).to_array(),
            corner_radii: [8.0; 4],
            border_color: c("#7aa2f7").with_alpha(0.5).to_array(),
            border_width: 1.0,
            gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
            shadow_color: [0.0; 4],
            shadow_offset: [0.0; 2],
            shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
        });
        glyphs.extend(tr.prepare_text(label, bx + 20.0, by + 12.0, 13.0, c("#c0caf5"), None));

        if !self.open { return; }

        // Dropdown container
        let dropdown_h = self.items.len() as f32 * 40.0 + 16.0;
        rects.push(RectInstance {
            rect: [bx, by + 48.0, bw, dropdown_h],
            fill_color: c("#24283b").to_array(),
            corner_radii: [10.0; 4],
            border_color: c("#414868").to_array(),
            border_width: 1.0,
            gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
            shadow_color: Color::BLACK.with_alpha(0.4).to_array(),
            shadow_offset: [0.0, 4.0],
            shadow_params: [16.0, 4.0],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
        });

        // Items falling with physics
        for item in &self.items {
            let iy = by + 48.0 + 8.0 + item.y;
            let item_x = bx + 8.0;
            let item_w = bw - 16.0;

            // Hover detection
            let hovered = mx >= item_x && mx <= item_x + item_w
                && my >= iy && my <= iy + 36.0;

            let bg_alpha = if hovered { 0.15 } else { 0.0 };
            if bg_alpha > 0.0 {
                rects.push(RectInstance {
                    rect: [item_x, iy, item_w, 36.0],
                    fill_color: c("#7aa2f7").with_alpha(bg_alpha).to_array(),
                    corner_radii: [6.0; 4],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                    shadow_color: [0.0; 4],
                    shadow_offset: [0.0; 2],
                    shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                });
            }

            let text_color = if hovered { c("#c0caf5") } else { c("#9aa5ce") };
            glyphs.extend(tr.prepare_text(
                item.label, item_x + 12.0, iy + 10.0, 13.0, text_color, None,
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Effect 4: Fluid Menu Indicator
// ---------------------------------------------------------------------------
struct FluidMenu {
    items: Vec<&'static str>,
    active: usize,
    indicator_x: f32, // current animated x
    indicator_w: f32, // current animated width
    hover_idx: Option<usize>,
    // Spring state
    vel_x: f32,
    vel_w: f32,
}

impl FluidMenu {
    fn new() -> Self {
        Self {
            items: vec!["Home", "Products", "Pricing", "Blog", "Contact"],
            active: 0,
            indicator_x: 0.0,
            indicator_w: 50.0,
            hover_idx: None,
            vel_x: 0.0,
            vel_w: 0.0,
        }
    }

    fn item_rect(&self, idx: usize, area_x: f32) -> (f32, f32) {
        let start_x = area_x + 40.0;
        let item_w = 100.0;
        let gap = 8.0;
        let x = start_x + idx as f32 * (item_w + gap);
        (x, item_w)
    }

    fn update(&mut self, mx: f32, my: f32, dt: f32, area_x: f32, area_y: f32) {
        // Hit test menu items
        let menu_y = area_y + 100.0;
        self.hover_idx = None;
        for i in 0..self.items.len() {
            let (ix, iw) = self.item_rect(i, area_x);
            if mx >= ix && mx <= ix + iw && my >= menu_y && my <= menu_y + 40.0 {
                self.hover_idx = Some(i);
            }
        }

        // Target position: hover item or active item
        let target_idx = self.hover_idx.unwrap_or(self.active);
        let (target_x, target_w) = self.item_rect(target_idx, area_x);

        // Spring physics for position and width
        let stiffness = 300.0;
        let damping = 25.0;

        let dx = target_x - self.indicator_x;
        let dw = target_w - self.indicator_w;
        self.vel_x += (dx * stiffness - self.vel_x * damping) * dt;
        self.vel_w += (dw * stiffness - self.vel_w * damping) * dt;
        self.indicator_x += self.vel_x * dt;
        self.indicator_w += self.vel_w * dt;
    }

    fn set_active(&mut self, idx: usize) {
        self.active = idx;
    }

    fn render(
        &self, mx: f32, my: f32,
        rects: &mut Vec<RectInstance>, glyphs: &mut Vec<GlyphInstance>, tr: &mut TextRenderer,
        area_x: f32, area_y: f32,
    ) {
        let menu_y = area_y + 100.0;

        // Menu background
        let total_w = self.items.len() as f32 * 108.0 + 32.0;
        rects.push(RectInstance {
            rect: [area_x + 24.0, menu_y - 4.0, total_w, 48.0],
            fill_color: c("#1e1e30").to_array(),
            corner_radii: [12.0; 4],
            border_color: c("#414868").to_array(),
            border_width: 1.0,
            gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
            shadow_color: [0.0; 4],
            shadow_offset: [0.0; 2],
            shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
        });

        // Fluid indicator (the blob that flows between items)
        // Stretch effect: when moving, expand width slightly
        let moving = self.vel_x.abs() > 10.0;
        let stretch = if moving { (self.vel_x.abs() / 200.0).min(0.3) } else { 0.0 };
        let ind_w = self.indicator_w + self.indicator_w * stretch;
        let ind_x = self.indicator_x - self.indicator_w * stretch * 0.5;

        rects.push(RectInstance {
            rect: [ind_x, menu_y, ind_w, 40.0],
            fill_color: c("#7aa2f7").with_alpha(0.2).to_array(),
            corner_radii: [8.0; 4],
            border_color: c("#7aa2f7").with_alpha(0.4).to_array(),
            border_width: 1.0,
            gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
            shadow_color: c("#7aa2f7").with_alpha(0.15).to_array(),
            shadow_offset: [0.0; 2],
            shadow_params: [12.0, 2.0],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
        });

        // Menu items text
        for (i, &label) in self.items.iter().enumerate() {
            let (ix, iw) = self.item_rect(i, area_x);
            let is_active = i == self.active;
            let is_hover = self.hover_idx == Some(i);
            let text_color = if is_active || is_hover {
                c("#c0caf5")
            } else {
                c("#9aa5ce")
            };
            let text_w = label.len() as f32 * 7.5;
            glyphs.extend(tr.prepare_text(
                label, ix + (iw - text_w) / 2.0, menu_y + 12.0, 13.0, text_color, None,
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Main App
// ---------------------------------------------------------------------------
struct EffectsApp {
    window: Option<Arc<Window>>,
    renderer: Option<sabitori::GpuRenderer>,
    text_renderer: Option<TextRenderer>,
    last_frame: Instant,
    mouse_x: f32,
    mouse_y: f32,
    mouse_pressed: bool,
    win_w: f32,
    win_h: f32,
    time: f32,
    fps: f32,
    frame_count: u32,
    fps_timer: Instant,
    // Effects
    spotlight: SpotlightGlow,
    magnetic: MagneticButtons,
    gravity_dropdown: GravityDropdown,
    fluid_menu: FluidMenu,
    // Layout
    active_section: usize,
}

impl EffectsApp {
    fn new() -> Self {
        Self {
            window: None, renderer: None, text_renderer: None,
            last_frame: Instant::now(),
            mouse_x: 0.0, mouse_y: 0.0, mouse_pressed: false,
            win_w: 1200.0, win_h: 800.0, time: 0.0,
            fps: 0.0, frame_count: 0, fps_timer: Instant::now(),
            spotlight: SpotlightGlow::new(),
            magnetic: MagneticButtons::new(),
            gravity_dropdown: GravityDropdown::new(),
            fluid_menu: FluidMenu::new(),
            active_section: 0,
        }
    }

    fn section_y(&self, idx: usize) -> f32 {
        70.0 + idx as f32 * (self.win_h * 0.5 - 20.0)
    }
}

impl ApplicationHandler for EffectsApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() { return; }
        let attrs = WindowAttributes::default()
            .with_title("Sabitori — Practical GPU Effects")
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
                let pressed = state == ElementState::Pressed;
                self.mouse_pressed = pressed;

                if pressed {
                    // Gravity dropdown toggle
                    let grav_y = self.section_y(2);
                    let bx = 60.0;
                    let by = grav_y + 30.0;
                    if self.mouse_x >= bx && self.mouse_x <= bx + 200.0
                        && self.mouse_y >= by && self.mouse_y <= by + 40.0 {
                        self.gravity_dropdown.toggle();
                    }

                    // Fluid menu click
                    let menu_area_y = self.section_y(3);
                    if let Some(idx) = self.fluid_menu.hover_idx {
                        self.fluid_menu.set_active(idx);
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                let now = Instant::now();
                let dt = (now - self.last_frame).as_secs_f32().min(0.05);
                self.last_frame = now;
                self.time += dt;

                // Update
                self.magnetic.update(self.mouse_x, self.mouse_y, 0.0, self.section_y(1));
                self.gravity_dropdown.update(dt);
                self.fluid_menu.update(self.mouse_x, self.mouse_y, dt, 0.0, self.section_y(3));

                // FPS
                self.frame_count += 1;
                if self.fps_timer.elapsed().as_secs_f32() >= 0.5 {
                    self.fps = self.frame_count as f32 / self.fps_timer.elapsed().as_secs_f32();
                    self.frame_count = 0;
                    self.fps_timer = Instant::now();
                }

                let Some(mut renderer) = self.renderer.take() else { return; };
                let mut tr = self.text_renderer.take().unwrap();
                let mouse_x = self.mouse_x;
                let mouse_y = self.mouse_y;
                let time = self.time;
                let win_w = self.win_w;
                let win_h = self.win_h;
                let fps = self.fps;
                let s1y = self.section_y(0);
                let s2y = self.section_y(1);
                let s3y = self.section_y(2);
                let s4y = self.section_y(3);

                let mut rects = Vec::with_capacity(200);
                let mut glyphs = Vec::new();

                // Background
                rects.push(RectInstance {
                    rect: [0.0, 0.0, win_w, win_h * 4.0],
                    fill_color: c("#0f0f1a").to_array(),
                    corner_radii: [0.0; 4],
                    border_color: [0.0; 4], border_width: 0.0, gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4],
                    shadow_color: [0.0; 4], shadow_offset: [0.0; 2], shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                });

                // Header
                let header = format!("Sabitori GPU Effects | {:.0} FPS", fps);
                glyphs.extend(tr.prepare_text(&header, 20.0, 16.0, 16.0, c("#c0caf5"), None));
                glyphs.extend(tr.prepare_text(
                    "CSSでは不可能な実用UIエフェクト",
                    20.0, 40.0, 12.0, c("#9aa5ce"), None,
                ));

                // Section divider line
                let div_color = c("#414868");

                // --- Section 1: Spotlight Glow ---
                glyphs.extend(tr.prepare_text(
                    "① Spotlight Glow — カーソル追従ライティング（Stripe風）",
                    20.0, s1y - 20.0, 13.0, c("#7aa2f7"), None,
                ));
                self.spotlight.render(
                    mouse_x, mouse_y, time,
                    &mut rects, &mut glyphs, &mut tr, 0.0, s1y,
                );

                // --- Section 2: Magnetic Buttons ---
                rects.push(RectInstance {
                    rect: [0.0, s2y - 30.0, win_w, 1.0],
                    fill_color: div_color.to_array(),
                    corner_radii: [0.0; 4], border_color: [0.0; 4], border_width: 0.0,
                    gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4], shadow_color: [0.0; 4], shadow_offset: [0.0; 2], shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                });
                glyphs.extend(tr.prepare_text(
                    "② Magnetic Snap — ボタンがカーソルに吸い寄せられる",
                    20.0, s2y - 20.0, 13.0, c("#bb9af7"), None,
                ));
                self.magnetic.render(
                    mouse_x, mouse_y,
                    &mut rects, &mut glyphs, &mut tr, 0.0, s2y,
                );

                // --- Section 3: Gravity Dropdown ---
                rects.push(RectInstance {
                    rect: [0.0, s3y - 30.0, win_w, 1.0],
                    fill_color: div_color.to_array(),
                    corner_radii: [0.0; 4], border_color: [0.0; 4], border_width: 0.0,
                    gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4], shadow_color: [0.0; 4], shadow_offset: [0.0; 2], shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                });
                glyphs.extend(tr.prepare_text(
                    "③ Gravity Dropdown — メニュー項目が物理演算で落下",
                    20.0, s3y - 20.0, 13.0, c("#9ece6a"), None,
                ));
                self.gravity_dropdown.render(
                    mouse_x, mouse_y,
                    &mut rects, &mut glyphs, &mut tr, 0.0, s3y,
                );

                // --- Section 4: Fluid Menu ---
                rects.push(RectInstance {
                    rect: [0.0, s4y - 30.0, win_w, 1.0],
                    fill_color: div_color.to_array(),
                    corner_radii: [0.0; 4], border_color: [0.0; 4], border_width: 0.0,
                    gradient_angle: 0.0, rotation: 0.0, _pad0: 0.0, clip_rect: [0.0; 4], shadow_color: [0.0; 4], shadow_offset: [0.0; 2], shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                });
                glyphs.extend(tr.prepare_text(
                    "④ Fluid Menu — インジケーターが液体のように流れる",
                    20.0, s4y - 20.0, 13.0, c("#e0af68"), None,
                ));
                self.fluid_menu.render(
                    mouse_x, mouse_y,
                    &mut rects, &mut glyphs, &mut tr, 0.0, s4y,
                );

                // Render
                let device = renderer.device.clone();
                let queue = renderer.queue.clone();
                let _ = renderer.render_with(&rects, |pass, globals_bg| {
                    tr.render_glyphs(&device, &queue, &glyphs, pass, globals_bg);
                });
                self.renderer = Some(renderer);
                self.text_renderer = Some(tr);
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
    let mut app = EffectsApp::new();
    event_loop.run_app(&mut app).unwrap();
}
