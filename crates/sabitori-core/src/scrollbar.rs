//! スクロールバーの幾何 ── **描く側と掴む側が同じ1つを使う。**
//!
//! 帯は [`crate::build`] が描き、掴みはランタイムが受ける。この2つが別々に
//! 寸法を持つと、**掴んだ所と摘まんだ物が食い違う** ── 1px ずれれば端で掴め
//! なくなるし、`min` の扱いが違えば短い並びで丸ごとずれる。式はここにしか
//! 置かない。
//!
//! 消費側が自前で写して持っていた例が実際にある（kasane が窓の側で掴みを
//! 実装していた。写しがずれていないかを「描かれた矩形と突き合わせる」試しで
//! 留めるしかなかった）。**掴めるのはランタイムの仕事**なので引き取った。

use crate::geometry::Rect;

/// 描かれる帯の幅。
pub const BAR_W: f32 = 4.0;

/// 枠の右端から帯までの距離（帯の右端 = `rect.x + w - INSET + BAR_W`）。
pub const BAR_INSET: f32 = 6.0;

/// つまみの下限。**これを割ると長い並びで摘まめなくなる。**
pub const MIN_THUMB: f32 = 20.0;

/// 掴める幅の既定 ── 描かれる 4px を狙わせるのは細すぎて、掴み損ねると
/// 下の中身（一覧なら絵）が選ばれる。
pub const DEFAULT_LANE: f32 = 14.0;

/// つまみの (枠の上端からの距離, 高さ)。
///
/// 中身が収まっているなら帯は出ない（高さ＝枠いっぱい・動かない）。
pub fn thumb(track: f32, content: f32, scroll: f32) -> (f32, f32) {
    if track <= 0.0 || content <= track {
        return (0.0, track.max(0.0));
    }
    let thumb_h = (track / content * track).clamp(MIN_THUMB.min(track), track);
    let norm = (scroll / (content - track)).clamp(0.0, 1.0);
    (norm * (track - thumb_h), thumb_h)
}

/// [`thumb`] の逆 ── つまみの先頭を `top` に置いた時の縦位置。
///
/// **同じ丸め方で戻す。**別々に書くと、掴んで置いた所と描かれる所が食い違う。
pub fn scroll_for(top: f32, track: f32, content: f32) -> f32 {
    if track <= 0.0 || content <= track {
        return 0.0;
    }
    let (_, thumb_h) = thumb(track, content, 0.0);
    let travel = track - thumb_h;
    if travel <= 0.0 {
        return 0.0;
    }
    ((top / travel).clamp(0.0, 1.0) * (content - track)).max(0.0)
}

/// 掴める帯1本。[`crate::build::BuildResult::scroll_bars`] に並ぶ。
///
/// **組んだフレームの寸法**なので、掴んでいる間はこれを持ち続ける側が正しい
/// （読み込みで中身が伸びる一覧では、毎フレーム引き直すとつまみが指から逃げる）。
#[derive(Debug, Clone)]
pub struct ScrollBar {
    /// `.scroll(id)` に書いた id。
    pub id: String,
    /// コンテナの矩形（画面座標）。
    pub rect: Rect,
    /// 中身の高さ。
    pub content: f32,
    /// 掴める幅（右端から内側へ）。
    pub lane: f32,
}

impl ScrollBar {
    /// つまみの (枠の上端からの距離, 高さ)。
    pub fn thumb(&self, scroll: f32) -> (f32, f32) {
        thumb(self.rect.size.height, self.content, scroll)
    }

    /// 掴める帯の中か。**縦だけ** ── 横の帯を掴む話は、横に流れる面
    /// （コマの帯など）が自分でコマを掴むので取り合いになる。
    pub fn lane_has(&self, x: f32, y: f32) -> bool {
        lane_has(self.rect, self.lane, x, y)
    }
}

/// [`ScrollBar::lane_has`] の、組み立て前に呼べる版。
///
/// [`crate::build::BuildResult::scroll_bar_id_at`] が使う ── 指が動くたびに
/// `ScrollBar` を組むと、1 移動につき面の数だけ `String` を作ることになる。
pub fn lane_has(rect: Rect, lane: f32, x: f32, y: f32) -> bool {
    let (rx, ry) = (rect.origin.x, rect.origin.y);
    let (rw, rh) = (rect.size.width, rect.size.height);
    x >= rx + rw - lane && x <= rx + rw && y >= ry && y <= ry + rh
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 5分の1の中身なら、つまみも5分の1。
    #[test]
    fn the_thumb_is_the_viewports_share_of_the_content() {
        assert_eq!(thumb(200.0, 1000.0, 0.0), (0.0, 40.0));
        let (top, h) = thumb(200.0, 1000.0, 800.0);
        assert_eq!(top + h, 200.0, "端まで送ってもつまみが枠から出る");
        let (top, _) = thumb(200.0, 1000.0, 400.0);
        assert!((top - 80.0).abs() < 0.01, "{top}");
    }

    /// 中身が収まっているなら動かない。
    #[test]
    fn a_pane_that_fits_has_nowhere_to_slide() {
        assert_eq!(thumb(200.0, 100.0, 0.0), (0.0, 200.0));
        assert_eq!(scroll_for(50.0, 200.0, 100.0), 0.0);
    }

    /// **20px を割らない** ── 長い並びでも摘まめる大きさが要る。
    #[test]
    fn the_thumb_never_shrinks_past_a_grabbable_size() {
        let (_, h) = thumb(200.0, 100_000.0, 0.0);
        assert_eq!(h, MIN_THUMB);
    }

    /// 置いた所へ戻る ── [`thumb`] と往復して同じ値。
    #[test]
    fn putting_the_thumb_back_lands_on_the_same_scroll() {
        for scroll in [0.0, 137.0, 400.0, 799.0, 800.0] {
            let (top, _) = thumb(200.0, 1000.0, scroll);
            let back = scroll_for(top, 200.0, 1000.0);
            assert!((back - scroll).abs() < 0.5, "{scroll} -> {top} -> {back}");
        }
    }

    /// 端の外は端で止まる。
    #[test]
    fn the_ends_hold() {
        assert_eq!(scroll_for(-500.0, 200.0, 1000.0), 0.0);
        assert_eq!(scroll_for(9999.0, 200.0, 1000.0), 800.0);
    }

    /// 掴める幅は描かれている帯より広い。
    #[test]
    fn the_lane_is_wider_than_the_painted_bar() {
        let bar = ScrollBar {
            id: "x".into(),
            rect: Rect::new(100.0, 50.0, 200.0, 400.0),
            content: 2000.0,
            lane: DEFAULT_LANE,
        };
        assert!(bar.lane_has(298.0, 60.0), "描かれている帯の上");
        assert!(bar.lane_has(289.0, 60.0), "その内側も掴める");
        assert!(!bar.lane_has(280.0, 60.0), "そこは中身");
        assert!(!bar.lane_has(298.0, 40.0), "枠の外");
        assert!(!bar.lane_has(298.0, 460.0), "枠の外");
    }
}
