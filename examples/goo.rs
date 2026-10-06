//! Goo — two rounded rects fused by an SDF smooth union.
//!
//! - Move the mouse: the panel follows the pointer and fuses with the pill
//!   at the top once the gap between them drops below half the smoothing
//!   radius `k`.
//! - Scroll: change the smoothing radius `k`.
//! - Click: toggle "drip" mode — the panel springs out of the pill and back,
//!   with `k` fading to 0 as it opens (the popup morph this was built for).
//!
//! The rows inside the panel are ordinary rects drawn *after* the goo, to
//! show that goo paints in tree order like a background.

use sabitori::element::*;
use sabitori::*;

const PILL: (f32, f32) = (132.0, 34.0);
const PANEL: (f32, f32) = (300.0, 196.0);
const PANEL_GAP: f32 = 14.0;

struct GooDemo {
    pointer: (f32, f32),
    /// Smoothing radius used in follow mode (logical px).
    k: f32,
    /// Drip mode: spring-driven open progress `p` toward `target`.
    drip: bool,
    p: f32,
    v: f32,
    target: f32,
}

impl GooDemo {
    /// The fixed pill, centered in a window `width` wide.
    fn pill_rect(width: f32) -> Rect {
        Rect::new((width - PILL.0) * 0.5, 22.0, PILL.0, PILL.1)
    }

    /// Panel rect and smoothing radius for this frame.
    fn panel(&self, width: f32) -> (Rect, f32) {
        if !self.drip {
            let (x, y) = self.pointer;
            return (Rect::new(x - PANEL.0 * 0.5, y - PANEL.1 * 0.5, PANEL.0, PANEL.1), self.k);
        }
        // Grow from the pill's footprint to the open panel hanging below it.
        let pill = Self::pill_rect(width);
        let p = self.p.max(0.0);
        let lerp = |a: f32, b: f32| a + (b - a) * p;
        let open_x = (width - PANEL.0) * 0.5;
        let open_y = pill.origin.y + pill.size.height + PANEL_GAP;
        let rect = Rect::new(
            lerp(pill.origin.x + 8.0, open_x),
            lerp(pill.origin.y + 4.0, open_y),
            lerp(pill.size.width - 16.0, PANEL.0),
            lerp(pill.size.height - 8.0, PANEL.1),
        );
        // Fat neck while it leaves the pill, gone once it has settled.
        let k = 48.0 * (1.0 - self.p.clamp(0.0, 1.0)).powf(1.5);
        (rect, k)
    }
}

impl DeclarativeApp for GooDemo {
    fn title(&self) -> &str { "Sabitori — goo" }
    fn size(&self) -> (f32, f32) { (900.0, 640.0) }

    fn on_pointer_move(&mut self, x: f32, y: f32) {
        self.pointer = (x, y);
    }

    fn on_scroll(&mut self, delta_y: f32) {
        self.k = (self.k + delta_y * 0.25).clamp(0.0, 160.0);
    }

    fn on_click(&mut self, _id: &str) {
        if self.drip {
            self.target = if self.target > 0.5 { 0.0 } else { 1.0 };
        } else {
            self.drip = true;
            self.p = 0.0;
            self.v = 0.0;
            self.target = 1.0;
        }
    }

    fn tick(&mut self, dt: f32) {
        if !self.drip {
            return;
        }
        // Slightly underdamped spring, like matcha's popup_anim.
        let (stiffness, damping) = (180.0, 20.0);
        let dt = dt.min(1.0 / 30.0);
        self.v += ((self.target - self.p) * stiffness - self.v * damping) * dt;
        self.p += self.v * dt;
        // Back in the pill and at rest: return to follow mode.
        if self.target == 0.0 && self.p.abs() < 0.002 && self.v.abs() < 0.01 {
            self.drip = false;
        }
    }

    fn is_animating(&self) -> bool {
        self.drip && ((self.target - self.p).abs() > 0.0005 || self.v.abs() > 0.0005)
    }

    fn view(&self, ctx: &ViewContext) -> Element {
        let bg = Color::from_hex("#15161e");
        let bar = Color::from_hex("#1f2335");
        let goo_color = Color::from_hex("#3b4261");
        let row = Color::from_hex("#565f89");
        let text_c = Color::from_hex("#c0caf5");
        let dim = Color::from_hex("#7a82a8");

        let pill = Self::pill_rect(ctx.width);
        let (panel, k) = self.panel(ctx.width);

        let mut rows = Vec::new();
        if panel.size.width > 120.0 && panel.size.height > 80.0 {
            for i in 0..3 {
                let y = panel.origin.y + 18.0 + i as f32 * 40.0;
                if y + 30.0 > panel.origin.y + panel.size.height - 10.0 {
                    break;
                }
                rows.push(
                    div()
                        .pos(panel.origin.x + 14.0, y)
                        .w(Px(panel.size.width - 28.0))
                        .h(Px(30.0))
                        .bg(row.with_alpha(0.55))
                        .rounded_px(8.0)
                        .px_pad(Px(10.0))
                        .flex_row()
                        .items_center()
                        .child(text(format!("row {} — drawn after the goo", i + 1)).font_size(13.0).color(text_c)),
                );
            }
        }

        let mode = if self.drip { "drip (click to toggle)" } else { "follow (click: drip)" };
        div()
            .id("stage")
            .w(Px(ctx.width))
            .h(Px(ctx.height))
            .bg(bg)
            .child(div().pos(0.0, 0.0).w(Px(ctx.width)).h(Px(78.0)).bg(bar))
            .child(
                goo(pill, panel)
                    .pos(0.0, 0.0)
                    .goo_radii(PILL.1 * 0.5, 14.0)
                    .goo_smooth(k)
                    .goo_color(goo_color),
            )
            .children(rows)
            .child(
                div()
                    .pos(pill.origin.x, pill.origin.y)
                    .w(Px(pill.size.width))
                    .h(Px(pill.size.height))
                    .flex_row()
                    .items_center()
                    .justify_center()
                    .child(text("12:34").font_size(15.0).color(text_c)),
            )
            .child(
                text(format!("mode: {mode}    k = {k:.0}px (scroll)"))
                    .pos(16.0, ctx.height - 32.0)
                    .font_size(13.0)
                    .color(dim),
            )
    }
}

fn main() {
    sabitori::run_declarative(GooDemo {
        pointer: (450.0, 300.0),
        k: 40.0,
        drip: false,
        p: 0.0,
        v: 0.0,
        target: 0.0,
    });
}
