//! 宣言的な分割ペイン。 仕切りをドラッグして比率を変える。
//!
//! ```ignore
//! // view():
//! split_pane(ctx, "main", &self.split, sidebar, editor)
//!
//! // on_input(): 仕切りのドラッグを state に渡す
//! self.split.on_input("main", event, ctx_width);
//! ```
//!
//! ## 0.4.0 での作り直し
//!
//! 旧 `SplitPane` は `new(bounds: Rect, ..)` を受け取って `first_pane()` /
//! `second_pane()` / `divider_rect()` を **`Rect` で返す**幾何オラクルだった。
//! 呼び出し側がその矩形を見て自分で要素を絶対配置する前提で、 `view()` の
//! flex ツリーとは噛み合わず、 repo 内の使用箇所は 0 だった。
//!
//! 宣言版が持つのは比率とドラッグ状態だけ。 実際の配置は flex がやる。

use sabitori_core::element::{div, Cursor, Element, Px, Role};
use sabitori_core::{Color, ViewContext};
use sabitori_input::InputEvent;

/// 分割の向き。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum SplitDirection {
    /// 左右に分ける (仕切りは縦線)。
    #[default]
    Horizontal,
    /// 上下に分ける (仕切りは横線)。
    Vertical,
}

/// 仕切りの太さ (px)。 当たり判定も見た目もこの幅。
pub const DIVIDER_PX: f32 = 6.0;

/// 分割ペインの状態。
#[derive(Clone, Debug)]
pub struct SplitPaneState {
    pub direction: SplitDirection,
    /// 1 枚目が占める割合 (0.0〜1.0)。
    ratio: f32,
    /// 1 枚目の最小サイズ (px)。
    pub min_first: f32,
    /// 2 枚目の最小サイズ (px)。
    pub min_second: f32,
    /// ドラッグ中か。 ドラッグ中は仕切りが強調される。
    dragging: bool,
}

impl Default for SplitPaneState {
    fn default() -> Self {
        Self::new(SplitDirection::Horizontal, 0.5)
    }
}

impl SplitPaneState {
    pub fn new(direction: SplitDirection, ratio: f32) -> Self {
        Self {
            direction,
            ratio: ratio.clamp(0.0, 1.0),
            min_first: 80.0,
            min_second: 80.0,
            dragging: false,
        }
    }

    pub fn ratio(&self) -> f32 {
        self.ratio
    }

    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// 比率を直接指定する。 `total` を渡すと最小サイズで丸める。
    pub fn set_ratio(&mut self, ratio: f32, total: f32) {
        self.ratio = clamp_ratio(ratio, total, self.min_first, self.min_second);
    }

    /// 仕切りの id。 `on_input` で自前に突き合わせたいとき用。
    pub fn divider_id(id: &str) -> String {
        format!("{id}::divider")
    }

    /// 仕切りのドラッグを処理する。 `total` は分割方向の全長 (px)。
    ///
    /// 押した位置が仕切りかどうかは `hovered` (= `ctx.hovered`) で判定する。
    /// 戻り値は「この event を消費したか」で、 `on_input` の戻り値にそのまま使える。
    pub fn on_input(
        &mut self,
        id: &str,
        event: &InputEvent,
        hovered: Option<&str>,
        total: f32,
    ) -> bool {
        let divider = Self::divider_id(id);
        match event {
            InputEvent::PointerPressed { .. } if hovered == Some(divider.as_str()) => {
                self.dragging = true;
                true
            }
            InputEvent::PointerMoved { position, .. } if self.dragging => {
                let along = match self.direction {
                    SplitDirection::Horizontal => position.x,
                    SplitDirection::Vertical => position.y,
                };
                if total > 0.0 {
                    self.set_ratio(along / total, total);
                }
                true
            }
            InputEvent::PointerReleased { .. } | InputEvent::PointerCancelled { .. }
                if self.dragging =>
            {
                self.dragging = false;
                true
            }
            _ => false,
        }
    }
}

/// 最小サイズを尊重して比率を丸める。 `total` が最小 2 枚ぶんに満たない場合は
/// そのまま 0.5 に寄せる (どう割っても最小を満たせないので、 潰れ方を均等にする)。
fn clamp_ratio(ratio: f32, total: f32, min_first: f32, min_second: f32) -> f32 {
    if total <= 0.0 {
        return ratio.clamp(0.0, 1.0);
    }
    if min_first + min_second >= total {
        return 0.5;
    }
    let lo = min_first / total;
    let hi = 1.0 - min_second / total;
    ratio.clamp(lo, hi)
}

/// 分割ペインの見た目。
#[derive(Clone, Debug)]
pub struct SplitPaneStyle {
    pub divider: Color,
    pub divider_hover: Color,
    pub divider_active: Color,
}

impl SplitPaneStyle {
    pub fn default_dark() -> Self {
        Self {
            divider: Color::from_hex("#2a2a44"),
            divider_hover: Color::from_hex("#3a3a66"),
            divider_active: Color::from_hex("#6c8cff"),
        }
    }
}

/// 2 枚のペインを仕切り付きで並べる。
///
/// 結果に `.flex_1()` なり `.w_full().h_full()` なりを繋いで、 親の中での
/// 大きさを決めること。
pub fn split_pane(
    ctx: &ViewContext,
    id: &str,
    state: &SplitPaneState,
    style: &SplitPaneStyle,
    first: Element,
    second: Element,
) -> Element {
    let divider_id = SplitPaneState::divider_id(id);
    let hovered = ctx.hovered.as_deref() == Some(divider_id.as_str());

    let color = if state.dragging {
        style.divider_active
    } else if hovered {
        style.divider_hover
    } else {
        style.divider
    };

    let horizontal = state.direction == SplitDirection::Horizontal;

    let divider = {
        let d = div()
            .id(&divider_id)
            .role(Role::Separator)
            .shrink(0.0)
            .bg(color);
        if horizontal {
            d.w(Px(DIVIDER_PX)).h_full().cursor(Cursor::ResizeEw)
        } else {
            d.h(Px(DIVIDER_PX)).w_full().cursor(Cursor::ResizeNs)
        }
    };

    // 比率は flex_grow に載せる。 taffy が実寸を出すので、 widget 側は px を知らない。
    let a = first.grow(state.ratio).shrink(1.0).basis(Px(0.0)).overflow_hidden();
    let b = second.grow(1.0 - state.ratio).shrink(1.0).basis(Px(0.0)).overflow_hidden();

    let root = div().id(id).role(Role::Group).children([a, divider, b]);
    if horizontal {
        root.flex_row()
    } else {
        root.flex_col()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 最小サイズを割り込まないこと。 仕切りを端まで引いてもペインが消えない。
    #[test]
    fn the_ratio_respects_minimum_pane_sizes() {
        let mut s = SplitPaneState::new(SplitDirection::Horizontal, 0.5);
        s.min_first = 100.0;
        s.min_second = 200.0;

        s.set_ratio(0.0, 1000.0);
        assert_eq!(s.ratio(), 0.1, "1 枚目は 100px を下回らない");

        s.set_ratio(1.0, 1000.0);
        assert_eq!(s.ratio(), 0.8, "2 枚目は 200px を下回らない");
    }

    /// 全長が最小 2 枚ぶんに満たないときは均等割り (0 除算も無限ループも無し)。
    #[test]
    fn a_too_small_total_falls_back_to_an_even_split() {
        let mut s = SplitPaneState::new(SplitDirection::Horizontal, 0.9);
        s.min_first = 100.0;
        s.min_second = 100.0;
        s.set_ratio(0.9, 150.0);
        assert_eq!(s.ratio(), 0.5);
    }

    /// ドラッグは press → move → release で完結し、 それ以外は消費しないこと。
    ///
    /// 仕切りの上に居ない press まで消費すると、 **中身のクリックが死ぬ**。
    #[test]
    fn dragging_only_consumes_events_it_owns() {
        let mut s = SplitPaneState::new(SplitDirection::Horizontal, 0.5);
        let press = InputEvent::PointerPressed {
            id: sabitori_input::MOUSE_POINTER_ID,
            kind: sabitori_input::PointerKind::Mouse,
            position: sabitori_core::Point::new(0.0, 0.0),
            button: Some(sabitori_input::MouseButton::Left),
            modifiers: sabitori_input::Modifiers::default(),
        };

        // 仕切りの上に居ないので消費しない。
        assert!(!s.on_input("sp", &press, Some("something-else"), 1000.0));
        assert!(!s.is_dragging());

        // 仕切りの上なら掴む。
        assert!(s.on_input("sp", &press, Some("sp::divider"), 1000.0));
        assert!(s.is_dragging());

        let mv = InputEvent::PointerMoved {
            id: sabitori_input::MOUSE_POINTER_ID,
            kind: sabitori_input::PointerKind::Mouse,
            position: sabitori_core::Point::new(300.0, 0.0),
            modifiers: sabitori_input::Modifiers::default(),
        };
        assert!(s.on_input("sp", &mv, None, 1000.0));
        assert_eq!(s.ratio(), 0.3, "掴んだ後は hovered に関係なく追従する");

        let up = InputEvent::PointerReleased {
            id: sabitori_input::MOUSE_POINTER_ID,
            kind: sabitori_input::PointerKind::Mouse,
            position: sabitori_core::Point::new(300.0, 0.0),
            button: Some(sabitori_input::MouseButton::Left),
            modifiers: sabitori_input::Modifiers::default(),
        };
        assert!(s.on_input("sp", &up, None, 1000.0));
        assert!(!s.is_dragging());

        // 離した後の move はもう消費しない。
        assert!(!s.on_input("sp", &mv, None, 1000.0));
    }
}
