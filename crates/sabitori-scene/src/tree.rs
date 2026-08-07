use sabitori_core::Point;
use slotmap::SlotMap;

use crate::node::{NodeId, NodeStyle, UiNode};
use sabitori_core::Rect;

pub struct NodeTree {
    pub nodes: SlotMap<NodeId, UiNode>,
    pub root_children: Vec<NodeId>,
}

impl NodeTree {
    pub fn new() -> Self {
        Self {
            nodes: SlotMap::with_key(),
            root_children: Vec::new(),
        }
    }

    pub fn add(&mut self, bounds: Rect, style: NodeStyle) -> NodeId {
        let node = UiNode::new(bounds, style);
        let id = self.nodes.insert(node);
        self.root_children.push(id);
        id
    }

    pub fn add_child(&mut self, parent: NodeId, bounds: Rect, style: NodeStyle) -> NodeId {
        let node = UiNode::new(bounds, style);
        let id = self.nodes.insert(node);
        if let Some(parent_node) = self.nodes.get_mut(parent) {
            parent_node.children.push(id);
        }
        id
    }

    /// Set hover style for a node.
    pub fn set_hover_style(&mut self, id: NodeId, style: NodeStyle) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.hover_style = Some(style);
            node.interactive = true;
        }
    }

    /// Set active (pressed) style for a node.
    pub fn set_active_style(&mut self, id: NodeId, style: NodeStyle) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.active_style = Some(style);
            node.interactive = true;
        }
    }

    /// Set click handler for a node.
    pub fn set_on_click(&mut self, id: NodeId, handler: impl FnMut() + 'static) {
        if let Some(node) = self.nodes.get_mut(id) {
            node.on_click = Some(Box::new(handler));
            node.interactive = true;
        }
    }

    /// Hit test: find the topmost interactive node at a point.
    /// Walks back-to-front (last child = topmost).
    pub fn hit_test(&self, point: Point) -> Option<NodeId> {
        self.hit_test_children(&self.root_children, point)
    }

    fn hit_test_children(&self, children: &[NodeId], point: Point) -> Option<NodeId> {
        // Reverse order: last drawn = topmost
        for &id in children.iter().rev() {
            if let Some(node) = self.nodes.get(id) {
                // Check children first (they're on top)
                if let Some(child_hit) = self.hit_test_children(&node.children, point) {
                    return Some(child_hit);
                }
                // Then check this node
                if node.interactive && node.hit_test(point) {
                    return Some(id);
                }
            }
        }
        None
    }

    /// Update hover state. Returns true if state changed.
    pub fn update_hover(&mut self, hovered_id: Option<NodeId>) -> bool {
        let mut changed = false;
        for (id, node) in self.nodes.iter_mut() {
            let should_hover = hovered_id == Some(id);
            if node.hovered != should_hover {
                node.hovered = should_hover;
                changed = true;
            }
        }
        changed
    }

    /// Set pressed state on a specific node.
    pub fn set_pressed(&mut self, id: Option<NodeId>, pressed: bool) {
        for (node_id, node) in self.nodes.iter_mut() {
            let should_press = id == Some(node_id) && pressed;
            node.pressed = should_press;
        }
    }

    /// Fire click handler on a node.
    pub fn fire_click(&mut self, id: NodeId) {
        if let Some(node) = self.nodes.get_mut(id) {
            if let Some(ref mut handler) = node.on_click {
                handler();
            }
        }
    }

    /// Animate all nodes. Call once per frame.
    pub fn animate(&mut self, dt: f32) {
        for (_, node) in self.nodes.iter_mut() {
            node.animate(dt);
        }
    }
}

impl Default for NodeTree {
    fn default() -> Self {
        Self::new()
    }
}
