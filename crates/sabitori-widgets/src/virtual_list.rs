//! VirtualList — 仮想スクロールリスト。可視行のみ描画。
//!
//! ```ignore
//! VirtualList::new("my-list", &items)
//!     .item_height(36.0)
//!     .render(|item, index| {
//!         div().h(Px(36.0)).children([text(&item.name)])
//!     })
//!     .build(ctx, scroll_y)
//! ```

use sabitori_core::*;

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

    pub fn build(self, ctx: &ViewContext, scroll_y: f32) -> Element {
        let total = self.items.len();
        let ih = self.item_height;
        let render = self.render_item.expect("VirtualList: render() not set");

        // scroll_y から可視範囲を直接計算（overflow_scroll 不使用）
        let first = (scroll_y / ih).floor().max(0.0) as usize;
        let viewport_h = ctx.height;
        let count = (viewport_h / ih).ceil() as usize + 3;
        let end = (first + count).min(total);

        let mut children = Vec::with_capacity(end - first);
        for i in first..end {
            children.push(render(&self.items[i], i));
        }

        div().id(self.id)
            .flex_1()
            .flex_col()
            .children(children)
    }
}
