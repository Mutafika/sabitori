//! 仮想スクロールリスト。 可視行だけを [`Element`] にする。
//!
//! ```ignore
//! // view():
//! virtual_list(ctx, "log", &self.lines, 20.0, |line, _i| {
//!     text(line).font_size(13.0)
//! })
//! ```
//!
//! スクロール位置はランタイムが `.scroll(id)` で持つ。 アプリが `scroll_y` を
//! 持ち回す必要は無い。
//!
//! ## 0.4.0 で直したこと
//!
//! 旧 `VirtualList::build(ctx, scroll_y)` には 2 つ問題があった。
//!
//! 1. **`scroll_y` を呼び出し側から受け取っていた**のに、 その値をどこから
//!    取るのかを doc が一言も書いていなかった。 ランタイム管理のスクロール
//!    (`ctx.scroll_info(id)`) と繋がっていないので、 素直に書くと 0 のまま
//!    動かない。
//! 2. **可視範囲を `ctx.height` から計算していた。** これはウィンドウの高さで、
//!    リストを置いた入れ物の高さではない。 サイドパネルの中に入れると、
//!    実際の 3〜4 倍の行を作った上に、 スクロールすると下端で行が尽きた。
//!
//! どちらも `ctx.visible_range(id, item_height)` に寄せて解消した。 これは
//! ランタイムが実測した viewport とスクロール位置から範囲を返す。
//! 上下に空の spacer を積むので、 スクロールバーの長さも実データに合う。

use sabitori_core::element::{div, Element, Px, Role};
use sabitori_core::ViewContext;

/// 仮想リストを組み立てる。
///
/// * `id` — スクロールコンテナの id。 `.scroll(id)` が付くのでランタイムが
///   スクロール位置を持つ。
/// * `item_height` — 1 行の高さ (px)。 **全行が同じ高さである前提**で、
///   ここがずれるとスクロール位置と描画位置がずれる。
/// * `render` — `(item, index)` から 1 行の [`Element`] を作る。
///
/// 高さは呼び出し側が決める (結果に `.flex_1()` か `.h(Px(..))` を繋ぐ)。
pub fn virtual_list<T>(
    ctx: &ViewContext,
    id: &str,
    items: &[T],
    item_height: f32,
    render: impl Fn(&T, usize) -> Element,
) -> Element {
    let (first, count) = ctx.visible_range(id, item_height);
    let end = (first + count).min(items.len());
    let first = first.min(end);

    // 見えていない行のぶんは高さだけ確保する。 これが無いと、 スクロール量が
    // 実データの長さと合わず、 少し動かしただけで最下部に着いてしまう。
    let spacer_top = first as f32 * item_height;
    let spacer_bottom = (items.len().saturating_sub(end)) as f32 * item_height;

    let mut children = Vec::with_capacity(end - first + 2);
    if spacer_top > 0.0 {
        children.push(div().h(Px(spacer_top)).shrink(0.0));
    }
    for (i, item) in items.iter().enumerate().take(end).skip(first) {
        children.push(render(item, i).shrink(0.0));
    }
    if spacer_bottom > 0.0 {
        children.push(div().h(Px(spacer_bottom)).shrink(0.0));
    }

    div()
        .id(id)
        .role(Role::List)
        .scroll(id)
        .flex_col()
        .children(children)
}

/// ビルダー版。 [`virtual_list`] と同じものを、 引数が多いときに読みやすい形で
/// 書けるようにしたもの。
pub struct VirtualList<'a, T> {
    id: &'a str,
    items: &'a [T],
    item_height: f32,
    render_item: Option<Box<dyn Fn(&T, usize) -> Element + 'a>>,
}

impl<'a, T> VirtualList<'a, T> {
    pub fn new(id: &'a str, items: &'a [T]) -> Self {
        Self {
            id,
            items,
            item_height: 32.0,
            render_item: None,
        }
    }

    pub fn item_height(mut self, h: f32) -> Self {
        self.item_height = h;
        self
    }

    pub fn render(mut self, f: impl Fn(&T, usize) -> Element + 'a) -> Self {
        self.render_item = Some(Box::new(f));
        self
    }

    /// 組み立てる。 `render()` を呼んでいなければ panic する。
    pub fn build(self, ctx: &ViewContext) -> Element {
        let render = self.render_item.expect("VirtualList: render() not set");
        virtual_list(ctx, self.id, self.items, self.item_height, render)
    }
}
