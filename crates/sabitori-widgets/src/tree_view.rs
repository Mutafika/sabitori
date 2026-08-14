//! 宣言的な木構造ビュー。
//!
//! ```ignore
//! // view():
//! tree_view(ctx, "files", &self.tree, &TreeViewStyle::default_dark())
//!
//! // on_click():
//! if let Some(row) = tree_clicked_row("files", id) { self.tree.toggle_row(row); }
//! ```
//!
//! ## 0.4.0 での作り直し
//!
//! [`TreeNode`] のデータ構造はそのまま。 変わったのは 2 点:
//!
//! * `TreeView` が持っていた `hover_index` / `hover_anim` を削除した。 hover は
//!   ランタイムが `ctx.hovered` で持っているので、 二重に持つと必ずずれる。
//! * **開閉が label 一致で行われていた** (`toggle_at_path` が木を舐めて最初に
//!   label が一致したノードを開閉していた)。 同じ名前のノードが 2 つあると
//!   別のノードが開く。 展開位置の添字で辿る形に直した。

use sabitori_core::element::{div, text, Element, Px, Role};
use sabitori_core::{Color, ViewContext};

/// A node in a tree view.
pub struct TreeNode {
    pub label: String,
    pub icon: Option<String>,
    pub children: Vec<TreeNode>,
    pub expanded: bool,
    pub selected: bool,
    pub depth: usize,
}

impl TreeNode {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            icon: None,
            children: Vec::new(),
            expanded: false,
            selected: false,
            depth: 0,
        }
    }

    pub fn with_icon(mut self, icon: impl Into<String>) -> Self {
        self.icon = Some(icon.into());
        self
    }

    pub fn with_children(mut self, children: Vec<TreeNode>) -> Self {
        self.children = children;
        self
    }

    pub fn with_expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    pub fn is_leaf(&self) -> bool {
        self.children.is_empty()
    }

    pub fn toggle(&mut self) {
        if !self.is_leaf() {
            self.expanded = !self.expanded;
        }
    }

    /// Flatten the tree into visible items with their depths.
    pub fn flatten(&self) -> Vec<FlatTreeItem> {
        let mut items = Vec::new();
        self.flatten_recursive(&mut items, 0);
        items
    }

    fn flatten_recursive(&self, items: &mut Vec<FlatTreeItem>, depth: usize) {
        items.push(FlatTreeItem {
            label: self.label.clone(),
            icon: self.icon.clone(),
            depth,
            is_leaf: self.is_leaf(),
            expanded: self.expanded,
            selected: self.selected,
        });
        if self.expanded {
            for child in &self.children {
                child.flatten_recursive(items, depth + 1);
            }
        }
    }

    /// 展開状態での `row` 番目のノードを可変で借りる。
    ///
    /// **label ではなく位置で辿る。** 同じ名前のノードが複数あっても、
    /// 押した行のノードがそのまま返る。
    pub fn node_at_row(&mut self, row: usize) -> Option<&mut TreeNode> {
        let mut cursor = 0usize;
        Self::descend(self, row, &mut cursor)
    }

    fn descend<'a>(
        node: &'a mut TreeNode,
        target: usize,
        cursor: &mut usize,
    ) -> Option<&'a mut TreeNode> {
        if *cursor == target {
            return Some(node);
        }
        *cursor += 1;
        if !node.expanded {
            return None;
        }
        for child in &mut node.children {
            if let Some(found) = Self::descend(child, target, cursor) {
                return Some(found);
            }
        }
        None
    }

    /// `row` 番目のノードを開閉する。 葉なら何もしない。
    pub fn toggle_row(&mut self, row: usize) {
        if let Some(node) = self.node_at_row(row) {
            node.toggle();
        }
    }

    /// `row` 番目だけを選択状態にする。
    pub fn select_row(&mut self, row: usize) {
        Self::clear_selection(self);
        if let Some(node) = self.node_at_row(row) {
            node.selected = true;
        }
    }

    fn clear_selection(node: &mut TreeNode) {
        node.selected = false;
        for child in &mut node.children {
            Self::clear_selection(child);
        }
    }
}

/// Flattened tree item for rendering.
#[derive(Clone, Debug)]
pub struct FlatTreeItem {
    pub label: String,
    pub icon: Option<String>,
    pub depth: usize,
    pub is_leaf: bool,
    pub expanded: bool,
    pub selected: bool,
}

/// 木の見た目。
#[derive(Clone, Debug)]
pub struct TreeViewStyle {
    pub fg: Color,
    pub fg_selected: Color,
    pub bg_hover: Color,
    pub bg_selected: Color,
    pub row_height: f32,
    pub indent: f32,
    pub font_size: f32,
}

impl TreeViewStyle {
    pub fn default_dark() -> Self {
        Self {
            fg: Color::from_hex("#c8c8dc"),
            fg_selected: Color::from_hex("#ffffff"),
            bg_hover: Color::from_hex("#24243a"),
            bg_selected: Color::from_hex("#2a3a6a"),
            row_height: 26.0,
            indent: 16.0,
            font_size: 13.0,
        }
    }
}

/// 行 `row` の要素 id。
pub fn tree_row_id(id: &str, row: usize) -> String {
    format!("{id}::row:{row}")
}

/// クリックされた id が木の行なら、 その行番号。
pub fn tree_clicked_row(id: &str, clicked: &str) -> Option<usize> {
    clicked.strip_prefix(&format!("{id}::row:"))?.parse().ok()
}

/// 木を組み立てる。 `id` はスクロールコンテナの id でもある。
pub fn tree_view(ctx: &ViewContext, id: &str, root: &TreeNode, style: &TreeViewStyle) -> Element {
    let items = root.flatten();
    let (first, count) = ctx.visible_range(id, style.row_height);
    let end = (first + count).min(items.len());
    let first = first.min(end);

    let spacer_top = first as f32 * style.row_height;
    let spacer_bottom = (items.len().saturating_sub(end)) as f32 * style.row_height;

    let mut children = Vec::with_capacity(end - first + 2);
    if spacer_top > 0.0 {
        children.push(div().h(Px(spacer_top)).shrink(0.0));
    }
    for (row, item) in items.iter().enumerate().take(end).skip(first) {
        children.push(tree_row(ctx, id, row, item, style));
    }
    if spacer_bottom > 0.0 {
        children.push(div().h(Px(spacer_bottom)).shrink(0.0));
    }

    div()
        .id(id)
        .role(Role::Tree)
        .scroll(id)
        .flex_col()
        .children(children)
}

fn tree_row(
    ctx: &ViewContext,
    id: &str,
    row: usize,
    item: &FlatTreeItem,
    style: &TreeViewStyle,
) -> Element {
    let row_id = tree_row_id(id, row);
    let hovered = ctx.hovered.as_deref() == Some(row_id.as_str());

    let bg = if item.selected {
        style.bg_selected
    } else if hovered {
        style.bg_hover
    } else {
        Color::TRANSPARENT
    };
    let fg = if item.selected { style.fg_selected } else { style.fg };

    // 開閉マーク。 葉は場所だけ空けて、 ラベルの左端を揃える。
    let marker = if item.is_leaf {
        " "
    } else if item.expanded {
        "▾"
    } else {
        "▸"
    };

    let mut row_children = vec![
        div().w(Px(item.depth as f32 * style.indent)).shrink(0.0),
        text(marker)
            .font_size(style.font_size)
            .color(fg)
            .mono(),
    ];
    if let Some(icon) = &item.icon {
        row_children.push(text(icon.clone()).font_size(style.font_size).color(fg));
    }
    row_children.push(text(item.label.clone()).font_size(style.font_size).color(fg));

    div()
        .id(&row_id)
        .role(Role::TreeItem)
        .label(&item.label)
        // 深さは 1 始まり (根が 1)。 支援技術の階層表現に合わせる。
        .heading(item.depth as u8 + 1)
        .w_full()
        .h(Px(style.row_height))
        .shrink(0.0)
        .bg(bg)
        .flex_row()
        .items_center()
        .gap(4.0)
        .px_pad(Px(6.0))
        .children(row_children)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 同じ label のノードが 2 つあっても、 押した行のノードが開くこと。
    ///
    /// 0.4.0 より前の `toggle_item` は label で木を舐めていたので、
    /// **常に最初に見つかった方**が開いていた。
    #[test]
    fn toggling_a_row_hits_that_row_not_a_namesake() {
        let mut root = TreeNode::new("root")
            .with_children(vec![
                TreeNode::new("dup").with_children(vec![TreeNode::new("a")]),
                TreeNode::new("dup").with_children(vec![TreeNode::new("b")]),
            ])
            .with_expanded(true);

        // 行: 0=root, 1=dup(1つ目), 2=dup(2つ目)
        root.toggle_row(2);

        assert!(!root.children[0].expanded, "1 つ目は閉じたまま");
        assert!(root.children[1].expanded, "押した 2 つ目が開く");
    }

    /// 展開に応じて行番号がずれること (畳んだ子は行を消費しない)。
    #[test]
    fn row_numbering_follows_the_expanded_shape() {
        let mut root = TreeNode::new("root")
            .with_children(vec![
                TreeNode::new("first").with_children(vec![TreeNode::new("hidden")]),
                TreeNode::new("second"),
            ])
            .with_expanded(true);

        assert_eq!(root.flatten().len(), 3, "畳んでいる間は 3 行");
        assert_eq!(root.node_at_row(2).map(|n| n.label.clone()), Some("second".into()));

        root.toggle_row(1); // "first" を開く
        assert_eq!(root.flatten().len(), 4);
        assert_eq!(root.node_at_row(2).map(|n| n.label.clone()), Some("hidden".into()));
        assert_eq!(root.node_at_row(3).map(|n| n.label.clone()), Some("second".into()));
    }

    /// 選択は 1 つだけ。 前の選択が残らないこと。
    #[test]
    fn selecting_a_row_clears_the_previous_selection() {
        let mut root = TreeNode::new("root")
            .with_children(vec![TreeNode::new("a"), TreeNode::new("b")])
            .with_expanded(true);

        root.select_row(1);
        assert!(root.children[0].selected);
        root.select_row(2);
        assert!(!root.children[0].selected, "前の選択が外れる");
        assert!(root.children[1].selected);
    }

    /// 葉を開こうとしても何も起きないこと。
    #[test]
    fn toggling_a_leaf_is_a_no_op() {
        let mut root = TreeNode::new("root")
            .with_children(vec![TreeNode::new("leaf")])
            .with_expanded(true);
        root.toggle_row(1);
        assert!(!root.children[0].expanded);
    }

    /// 範囲外の行は無視すること (panic しない)。
    #[test]
    fn out_of_range_rows_are_ignored() {
        let mut root = TreeNode::new("root");
        root.toggle_row(99);
        root.select_row(99);
        assert!(root.node_at_row(99).is_none());
    }
}
