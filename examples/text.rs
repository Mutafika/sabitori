use std::sync::Arc;
use std::time::Instant;

use sabitori::{Color, GlyphInstance, RectInstance, TextRenderer};
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowAttributes, WindowId};

struct TextApp {
    window: Option<Arc<Window>>,
    renderer: Option<sabitori::GpuRenderer>,
    text_renderer: Option<TextRenderer>,
    last_frame: Instant,
}

impl ApplicationHandler for TextApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let attrs = WindowAttributes::default()
            .with_title("Sabitori - Text Rendering")
            .with_inner_size(winit::dpi::LogicalSize::new(1000.0, 900.0));

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
            WindowEvent::RedrawRequested => {
                let (Some(renderer), Some(text_renderer)) =
                    (self.renderer.as_mut(), self.text_renderer.as_mut())
                else {
                    return;
                };

                let scale = renderer.scale_factor;
                let w = renderer.surface_config.width as f32 / scale;
                let _h = renderer.surface_config.height as f32 / scale;

                // Background + cards
                let bg_color = Color::from_hex("#1a1a2e");
                let card_color = Color::from_hex("#22223a");
                let border_color = Color::from_hex("#3a3a55");
                let primary = Color::from_hex("#6c63ff");

                let mut rects = vec![
                    // Background
                    RectInstance {
                        rect: [0.0, 0.0, 2000.0, 2000.0],
                        fill_color: bg_color.to_array(),
                        corner_radii: [0.0; 4],
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        gradient_angle: 0.0,
                        rotation: 0.0,
                        _pad0: 0.0, clip_rect: [0.0; 4],
                        shadow_color: [0.0; 4],
                        shadow_offset: [0.0; 2],
                        shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                    },
                    // Main card
                    RectInstance {
                        rect: [40.0, 40.0, w - 80.0, 800.0],
                        fill_color: card_color.to_array(),
                        corner_radii: [16.0; 4],
                        border_color: border_color.to_array(),
                        border_width: 1.0,
                        gradient_angle: 0.0,
                        rotation: 0.0,
                        _pad0: 0.0, clip_rect: [0.0; 4],
                        shadow_color: Color::from_hex("#00000060").to_array(),
                        shadow_offset: [0.0, 4.0],
                        shadow_params: [16.0, 4.0],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                    },
                    // Accent bar
                    RectInstance {
                        rect: [40.0, 40.0, w - 80.0, 4.0],
                        fill_color: primary.to_array(),
                        corner_radii: [16.0, 16.0, 0.0, 0.0],
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        gradient_angle: 0.0,
                        rotation: 0.0,
                        _pad0: 0.0, clip_rect: [0.0; 4],
                        shadow_color: [0.0; 4],
                        shadow_offset: [0.0; 2],
                        shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                    },
                ];

                // Prepare text
                let mut all_glyphs: Vec<GlyphInstance> = Vec::new();

                let x = 80.0;
                let mut y = 80.0;

                // Title
                let title_glyphs = text_renderer.prepare_text(
                    "さびとり — Rust GPU GUI Framework",
                    x, y, 28.0,
                    Color::from_hex("#e8e8f0"),
                    Some(w - 160.0),
                );
                all_glyphs.extend_from_slice(&title_glyphs);
                y += 50.0;

                // Subtitle
                let sub_glyphs = text_renderer.prepare_text(
                    "美しい GPU テキストを Rust エコシステムに",
                    x, y, 16.0,
                    Color::from_hex("#9090a8"),
                    Some(w - 160.0),
                );
                all_glyphs.extend_from_slice(&sub_glyphs);
                y += 40.0;

                // Separator
                rects.push(RectInstance {
                    rect: [x, y, w - 160.0, 1.0],
                    fill_color: border_color.to_array(),
                    corner_radii: [0.0; 4],
                    border_color: [0.0; 4],
                    border_width: 0.0,
                    gradient_angle: 0.0,
                    rotation: 0.0,
                    _pad0: 0.0, clip_rect: [0.0; 4],
                    shadow_color: [0.0; 4],
                    shadow_offset: [0.0; 2],
                    shadow_params: [0.0; 2],
                    gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                });
                y += 20.0;

                // Feature list
                let features = [
                    "🎨 SDF角丸矩形 — ピクセルパーフェクトなアンチエイリアス",
                    "⚡ wgpuベース — Metal, Vulkan, DX12, WebGPU対応",
                    "📐 Taffyレイアウト — Flexbox + CSS Gridで自動配置",
                    "🔤 cosmic-text — 日本語・CJK完全対応のテキストレンダリング",
                    "🎭 テーマシステム — YAMLベースのホットリロード",
                    "✨ スプリング物理 — 自然なアニメーション",
                ];

                for feat in &features {
                    let feat_glyphs = text_renderer.prepare_text(
                        feat, x, y, 15.0,
                        Color::from_hex("#c8c8d8"),
                        Some(w - 160.0),
                    );
                    all_glyphs.extend_from_slice(&feat_glyphs);
                    y += 32.0;
                }

                y += 20.0;

                // Mixed text demo
                let mixed_text = "The quick brown fox jumps over the lazy dog. 素早い茶色の狐が怠惰な犬を飛び越える。ABCDEFGあいうえおカキクケコ漢字混在テスト。";
                let mixed_glyphs = text_renderer.prepare_text(
                    mixed_text, x, y, 14.0,
                    Color::from_hex("#b0b0c8"),
                    Some(w - 160.0),
                );
                all_glyphs.extend_from_slice(&mixed_glyphs);
                y += 60.0;

                // Code-style text
                rects.push(RectInstance {
                    rect: [x, y, w - 160.0, 80.0],
                    fill_color: Color::from_hex("#15152a").to_array(),
                    corner_radii: [8.0; 4],
                    border_color: border_color.to_array(),
                    border_width: 1.0,
                    gradient_angle: 0.0,
                    rotation: 0.0,
                    _pad0: 0.0, clip_rect: [0.0; 4],
                    shadow_color: [0.0; 4],
                    shadow_offset: [0.0; 2],
                    shadow_params: [0.0; 2],
                    gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                });

                let code = "fn main() {\n    sabitori::run(MyApp::new());\n}";
                let code_glyphs = text_renderer.prepare_text(
                    code, x + 16.0, y + 12.0, 14.0,
                    Color::from_hex("#4ade80"),
                    Some(w - 192.0),
                );
                all_glyphs.extend_from_slice(&code_glyphs);
                y += 120.0;

                // 回転注記 (DXF の TEXT のように、挿入点まわりに回す)。
                // ピボットは各テキストの原点 = 下のピンクの点。角度を変えても
                // 点は動かず、そこから文字列が生えるのが正しい挙動。
                let label_glyphs = text_renderer.prepare_text(
                    "回転注記 — ピボットはテキスト原点 (ピンクの点)",
                    x, y, 14.0,
                    Color::from_hex("#9090a8"),
                    None,
                );
                all_glyphs.extend_from_slice(&label_glyphs);
                y += 44.0;

                let pivot_color = Color::from_hex("#ff7ab6");
                for (i, deg) in [0.0_f32, 30.0, 60.0, 90.0, -30.0].iter().enumerate() {
                    let px = x + i as f32 * 170.0;
                    let py = y + 60.0;
                    let mut rotated = text_renderer.prepare_text(
                        &format!("{deg:.0}° 回転 TEXT"),
                        px, py, 15.0,
                        Color::from_hex("#ffd166"),
                        None,
                    );
                    // シェーピング済みの run を後から回す。cache key は角度を
                    // 含まないので、同じ文字列を別角度で描いても再シェーピング無し。
                    sabitori::rotate_glyphs(&mut rotated, (px, py), deg.to_radians());
                    all_glyphs.extend_from_slice(&rotated);

                    rects.push(RectInstance {
                        rect: [px - 2.5, py - 2.5, 5.0, 5.0],
                        fill_color: pivot_color.to_array(),
                        corner_radii: [2.5; 4],
                        border_color: [0.0; 4],
                        border_width: 0.0,
                        gradient_angle: 0.0,
                        rotation: 0.0,
                        _pad0: 0.0, clip_rect: [0.0; 4],
                        shadow_color: [0.0; 4],
                        shadow_offset: [0.0; 2],
                        shadow_params: [0.0; 2],
                        gradient_end_color: [0.0, 0.0, 0.0, 0.0],
                    });
                }

                // Render
                let device = renderer.device.clone();
                let queue = renderer.queue.clone();

                let _ = renderer.render_with(&rects, |pass, globals_bg| {
                    text_renderer.render_glyphs(&device, &queue, &all_glyphs, pass, globals_bg);
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
    let mut app = TextApp {
        window: None,
        renderer: None,
        text_renderer: None,
        last_frame: Instant::now(),
    };
    event_loop.run_app(&mut app).unwrap();
}
