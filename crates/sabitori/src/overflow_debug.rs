//! 親からはみ出した子を、debug ビルドの画面とログで知らせる
//! ([#95](https://github.com/Mutafika/sabitori/issues/95))。
//!
//! 窓を縮めたときの崩れは、描画もレイアウトも正常に終わるので何も出ない。
//! スクショを撮って目で探さない限り見つからなかった (sabitori-renta で 12 画面中
//! 6 画面)。検出はレイアウトの側 ([`BuildResult::overflows`]) が済ませているので、
//! ここは**見せ方だけ**を持つ:
//!
//! - はみ出した所に目印を描く (Flutter の overflow の縞に倣い、黄と黒の縞 +
//!   はみ出した部分の赤い網掛け)。窓を縮めていくだけで、どこが先に崩れるかが見える
//! - 同じ所 (経路) につき 1 回だけログに出す
//!
//! release ビルドでは何もしない。debug でも `SABITORI_OVERFLOW=0` で消せる。
//! Harness は描画の道を通らないので目印は入らない ([`crate::testing::Harness::overflows`]
//! で読む)。

use std::collections::HashSet;

use sabitori_core::build::{BuildResult, LayoutOverflow};
use sabitori_core::render_list::{RectDraw, RenderCommand};
use sabitori_core::{Color, Rect};

/// 縞の帯の太さ (px)。
const BAND: f32 = 4.0;
/// 縞 1 本の長さ (px)。
const STRIPE: f32 = 6.0;

const YELLOW: Color = Color::new(1.0, 0.84, 0.0, 1.0);
const BLACK: Color = Color::new(0.0, 0.0, 0.0, 1.0);
const TINT: Color = Color::new(1.0, 0.15, 0.1, 0.28);

pub(crate) struct OverflowDebug {
    enabled: bool,
    /// ログに出した経路。窓の幅を変えるたびに同じ所を出し直さない。
    seen: HashSet<String>,
}

impl OverflowDebug {
    /// debug ビルドなら有効。`SABITORI_OVERFLOW=0` で消せる。
    pub(crate) fn from_env() -> Self {
        let off = std::env::var("SABITORI_OVERFLOW").is_ok_and(|v| v == "0");
        Self { enabled: cfg!(debug_assertions) && !off, seen: HashSet::new() }
    }

    /// 描く直前に呼ぶ。`overlay` は外付けの木 (`overlay_view` など) の結果。
    ///
    /// 目印は `build.overlay_list` に積むので、中身の上に描かれる。
    pub(crate) fn flag(&mut self, build: &mut BuildResult, overlay: Option<&BuildResult>) {
        if !self.enabled {
            return;
        }
        // 地の木の目印は overlay の中身 (引き出し・modal・menu) より**下**に描く。
        // 最後に積むと、開いた引き出しの上に地の目印が透けて出る。外付けの木
        // (`overlay_view`) の目印は、その中身より上 = 最後に積む。
        let mut base_marks = Vec::new();
        for o in &build.overflows {
            self.log_once(o);
            base_marks.extend(markers(o));
        }
        build.overlay_list.commands.splice(0..0, base_marks);
        for o in overlay.into_iter().flat_map(|o| o.overflows.iter()) {
            self.log_once(o);
            build.overlay_list.commands.extend(markers(o));
        }
    }

    fn log_once(&mut self, o: &LayoutOverflow) {
        if self.seen.insert(o.path.clone()) {
            log::warn!("{}", describe(o));
        }
    }
}

/// ログの 1 行。どの辺が何 px 出たか。
pub(crate) fn describe(o: &LayoutOverflow) -> String {
    let sides: Vec<String> = [
        ("上", o.by.top),
        ("右", o.by.right),
        ("下", o.by.bottom),
        ("左", o.by.left),
    ]
    .iter()
    .filter(|(_, v)| *v > 0.0)
    .map(|(side, v)| format!("{side} {v:.0}px"))
    .collect();
    format!("親からはみ出している ({}): {}", sides.join(" / "), o.path)
}

/// 1 件ぶんの目印。はみ出した部分の網掛けと、親の辺に沿った縞。
pub(crate) fn markers(o: &LayoutOverflow) -> Vec<RenderCommand> {
    let (r, p) = (o.rect, o.parent);
    let (rl, rt, rr, rb) = (r.origin.x, r.origin.y, r.origin.x + r.size.width, r.origin.y + r.size.height);
    let (pl, pt, pr, pb) = (p.origin.x, p.origin.y, p.origin.x + p.size.width, p.origin.y + p.size.height);
    // 縞は親の辺の上、子と重なる長さだけ引く。
    let (h0, h1) = (rl.max(pl), rr.min(pr));
    let (v0, v1) = (rt.max(pt), rb.min(pb));
    let mut out = Vec::new();
    if o.by.right > 0.0 {
        out.push(fill(Rect::new(pr, rt, rr - pr, r.size.height), TINT));
        stripes(&mut out, Rect::new(pr - BAND, v0, BAND, v1 - v0), false);
    }
    if o.by.left > 0.0 {
        out.push(fill(Rect::new(rl, rt, pl - rl, r.size.height), TINT));
        stripes(&mut out, Rect::new(pl, v0, BAND, v1 - v0), false);
    }
    if o.by.bottom > 0.0 {
        out.push(fill(Rect::new(rl, pb, r.size.width, rb - pb), TINT));
        stripes(&mut out, Rect::new(h0, pb - BAND, h1 - h0, BAND), true);
    }
    if o.by.top > 0.0 {
        out.push(fill(Rect::new(rl, rt, r.size.width, pt - rt), TINT));
        stripes(&mut out, Rect::new(h0, pt, h1 - h0, BAND), true);
    }
    out
}

/// 帯を黄と黒の交互に塗る。`across` = 横に長い帯。
fn stripes(out: &mut Vec<RenderCommand>, band: Rect, across: bool) {
    let len = if across { band.size.width } else { band.size.height };
    if len <= 0.0 {
        return;
    }
    let mut at = 0.0;
    let mut i = 0;
    while at < len {
        let n = STRIPE.min(len - at);
        let seg = if across {
            Rect::new(band.origin.x + at, band.origin.y, n, band.size.height)
        } else {
            Rect::new(band.origin.x, band.origin.y + at, band.size.width, n)
        };
        out.push(fill(seg, if i % 2 == 0 { YELLOW } else { BLACK }));
        at += STRIPE;
        i += 1;
    }
}

fn fill(rect: Rect, color: Color) -> RenderCommand {
    RenderCommand::Rect(RectDraw { rect, fill_color: color, ..Default::default() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use sabitori_core::Edges;

    fn toolbar_overflow() -> LayoutOverflow {
        LayoutOverflow {
            id: None,
            path: "div[0] > #toolbar > div[1]".into(),
            rect: Rect::new(300.0, 0.0, 460.0, 40.0),
            parent: Rect::new(0.0, 0.0, 600.0, 40.0),
            by: Edges::new(0.0, 160.0, 0.0, 0.0),
        }
    }

    fn rects(cmds: &[RenderCommand]) -> Vec<RectDraw> {
        cmds.iter()
            .filter_map(|c| match c {
                RenderCommand::Rect(r) => Some(*r),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn the_part_past_the_edge_is_tinted_and_the_edge_is_striped() {
        let r = rects(&markers(&toolbar_overflow()));
        // 網掛けは親の右端 600 から子の右端 760 まで。
        assert_eq!(r[0].rect, Rect::new(600.0, 0.0, 160.0, 40.0));
        // 縞は親の右端の内側に、高さ 40 を 6px ずつ (最後は 4px)。
        let band = &r[1..];
        assert_eq!(band.len(), 7);
        assert!(band.iter().all(|s| s.rect.origin.x == 596.0 && s.rect.size.width == BAND));
        assert_eq!(band[0].fill_color, YELLOW);
        assert_eq!(band[1].fill_color, BLACK);
        assert_eq!(band[6].rect.size.height, 4.0);
    }

    #[test]
    fn each_path_is_logged_once_and_marked_every_frame() {
        let mut dbg = OverflowDebug { enabled: true, seen: HashSet::new() };
        let mut build = sabitori_core::build::build_tree(&sabitori_core::element::div(), 10.0, 10.0);
        build.overflows.push(toolbar_overflow());
        dbg.flag(&mut build, None);
        let first = build.overlay_list.commands.len();
        assert!(first > 0);
        dbg.flag(&mut build, None);
        assert_eq!(build.overlay_list.commands.len(), first * 2, "目印は毎フレーム描く");
        assert_eq!(dbg.seen.len(), 1, "ログは 1 回");
    }

    /// 地の木の目印は、開いている引き出しや modal (overlay の中身) より下。
    #[test]
    fn base_markers_go_under_the_overlay_content() {
        let mut dbg = OverflowDebug { enabled: true, seen: HashSet::new() };
        let mut build = sabitori_core::build::build_tree(&sabitori_core::element::div(), 10.0, 10.0);
        let drawer = fill(Rect::new(0.0, 0.0, 280.0, 500.0), Color::WHITE);
        build.overlay_list.commands.push(drawer);
        build.overflows.push(toolbar_overflow());
        dbg.flag(&mut build, None);
        let last = build.overlay_list.commands.last().unwrap();
        assert!(
            matches!(last, RenderCommand::Rect(r) if r.rect.size.width == 280.0),
            "引き出しが最後 (いちばん上) でない"
        );
    }

    #[test]
    fn disabled_draws_nothing() {
        let mut dbg = OverflowDebug { enabled: false, seen: HashSet::new() };
        let mut build = sabitori_core::build::build_tree(&sabitori_core::element::div(), 10.0, 10.0);
        build.overflows.push(toolbar_overflow());
        dbg.flag(&mut build, None);
        assert!(build.overlay_list.commands.is_empty());
    }

    #[test]
    fn the_log_line_names_the_sides_and_the_path() {
        assert_eq!(
            describe(&toolbar_overflow()),
            "親からはみ出している (右 160px): div[0] > #toolbar > div[1]"
        );
    }
}
