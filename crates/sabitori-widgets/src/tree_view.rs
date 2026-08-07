use sabitori_core::Color;
use sabitori_anim::{Animated, Spring};

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

/// Tree view widget.
pub struct TreeView {
    pub root: TreeNode,
    pub item_height: f32,
    pub indent: f32,
    pub hover_index: Option<usize>,
    pub hover_anim: Animated<f32>,
}

impl TreeView {
    pub fn new(root: TreeNode) -> Self {
        Self {
            root,
            item_height: 28.0,
            indent: 20.0,
            hover_index: None,
            hover_anim: Animated::new(0.0).with_spring(Spring::snappy()),
        }
    }

    pub fn visible_items(&self) -> Vec<FlatTreeItem> {
        self.root.flatten()
    }

    pub fn set_hover(&mut self, index: Option<usize>) {
        self.hover_index = index;
        if let Some(i) = index {
            self.hover_anim.set_target(i as f32);
        }
    }

    pub fn toggle_item(&mut self, index: usize) {
        let items = self.root.flatten();
        if let Some(item) = items.get(index) {
            if !item.is_leaf {
                // Find the node in the tree and toggle it
                self.toggle_at_path(&item.label);
            }
        }
    }

    fn toggle_at_path(&mut self, label: &str) {
        Self::toggle_recursive(&mut self.root, label);
    }

    fn toggle_recursive(node: &mut TreeNode, label: &str) -> bool {
        if node.label == label {
            node.toggle();
            return true;
        }
        for child in &mut node.children {
            if Self::toggle_recursive(child, label) {
                return true;
            }
        }
        false
    }

    pub fn tick(&mut self, dt: f32) {
        self.hover_anim.tick(dt);
    }
}
