use sabitori_core::Rect;
use sabitori_style::{
    AlignItems, Dimension, Display, FlexDirection, FlexWrap, JustifyContent, Overflow, Position,
    StyleProps,
};
use taffy::{
    AvailableSpace, LengthPercentage, LengthPercentageAuto, Size, Style, TaffyTree,
};

/// Opaque layout node ID.
pub type LayoutNodeId = taffy::NodeId;

/// Layout engine wrapping Taffy.
pub struct LayoutEngine {
    taffy: TaffyTree,
}

/// Result of layout computation for one element.
#[derive(Clone, Copy, Debug, Default)]
pub struct LayoutResult {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl LayoutResult {
    pub fn to_rect(self) -> Rect {
        Rect::new(self.x, self.y, self.width, self.height)
    }
}

impl LayoutEngine {
    pub fn new() -> Self {
        Self {
            taffy: TaffyTree::new(),
        }
    }

    /// Create a leaf node (no children).
    pub fn add_leaf(&mut self, style: &StyleProps) -> LayoutNodeId {
        self.taffy
            .new_leaf(convert_style(style))
            .expect("Failed to create leaf node")
    }

    /// Create a node with children.
    pub fn add_with_children(&mut self, style: &StyleProps, children: &[LayoutNodeId]) -> LayoutNodeId {
        self.taffy
            .new_with_children(convert_style(style), children)
            .expect("Failed to create node")
    }

    /// Update the style of an existing node.
    pub fn set_style(&mut self, node: LayoutNodeId, style: &StyleProps) {
        self.taffy
            .set_style(node, convert_style(style))
            .expect("Failed to set style");
    }

    /// Add a child to an existing node.
    pub fn add_child(&mut self, parent: LayoutNodeId, child: LayoutNodeId) {
        self.taffy
            .add_child(parent, child)
            .expect("Failed to add child");
    }

    /// Compute layout given available space.
    pub fn compute(&mut self, root: LayoutNodeId, width: f32, height: f32) {
        self.taffy
            .compute_layout(
                root,
                Size {
                    width: AvailableSpace::Definite(width),
                    height: AvailableSpace::Definite(height),
                },
            )
            .expect("Failed to compute layout");
    }

    /// Get the computed layout for a node (relative to parent).
    pub fn get_layout(&self, node: LayoutNodeId) -> LayoutResult {
        let layout = self.taffy.layout(node).expect("Node not found");
        LayoutResult {
            x: layout.location.x,
            y: layout.location.y,
            width: layout.size.width,
            height: layout.size.height,
        }
    }

    /// Get absolute position by walking up the tree.
    pub fn get_absolute_layout(&self, node: LayoutNodeId) -> LayoutResult {
        let mut result = self.get_layout(node);
        let mut current = node;
        while let Some(parent) = self.taffy.parent(current) {
            let parent_layout = self.taffy.layout(parent).expect("Parent not found");
            result.x += parent_layout.location.x;
            result.y += parent_layout.location.y;
            current = parent;
        }
        result
    }

    /// Get children of a node.
    pub fn children(&self, node: LayoutNodeId) -> Vec<LayoutNodeId> {
        self.taffy.children(node).unwrap_or_default()
    }

    /// Remove a node.
    pub fn remove(&mut self, node: LayoutNodeId) {
        let _ = self.taffy.remove(node);
    }

    /// Access the inner Taffy tree.
    pub fn taffy(&self) -> &TaffyTree {
        &self.taffy
    }

    pub fn taffy_mut(&mut self) -> &mut TaffyTree {
        &mut self.taffy
    }
}

impl Default for LayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert Sabitori StyleProps to Taffy Style.
fn convert_style(props: &StyleProps) -> Style {
    Style {
        display: match props.display {
            Display::Flex => taffy::Display::Flex,
            Display::Grid => taffy::Display::Grid,
            Display::None => taffy::Display::None,
        },
        position: match props.position {
            Position::Relative => taffy::Position::Relative,
            Position::Absolute => taffy::Position::Absolute,
        },
        flex_direction: match props.flex_direction {
            FlexDirection::Row => taffy::FlexDirection::Row,
            FlexDirection::Column => taffy::FlexDirection::Column,
            FlexDirection::RowReverse => taffy::FlexDirection::RowReverse,
            FlexDirection::ColumnReverse => taffy::FlexDirection::ColumnReverse,
        },
        flex_wrap: match props.flex_wrap {
            FlexWrap::NoWrap => taffy::FlexWrap::NoWrap,
            FlexWrap::Wrap => taffy::FlexWrap::Wrap,
            FlexWrap::WrapReverse => taffy::FlexWrap::WrapReverse,
        },
        align_items: Some(match props.align_items {
            AlignItems::Stretch => taffy::AlignItems::Stretch,
            AlignItems::Start => taffy::AlignItems::FlexStart,
            AlignItems::End => taffy::AlignItems::FlexEnd,
            AlignItems::Center => taffy::AlignItems::Center,
        }),
        justify_content: Some(match props.justify_content {
            JustifyContent::Start => taffy::JustifyContent::FlexStart,
            JustifyContent::End => taffy::JustifyContent::FlexEnd,
            JustifyContent::Center => taffy::JustifyContent::Center,
            JustifyContent::SpaceBetween => taffy::JustifyContent::SpaceBetween,
            JustifyContent::SpaceAround => taffy::JustifyContent::SpaceAround,
            JustifyContent::SpaceEvenly => taffy::JustifyContent::SpaceEvenly,
        }),
        flex_grow: props.flex_grow,
        flex_shrink: props.flex_shrink,
        gap: Size {
            width: length(props.gap),
            height: length(props.gap),
        },
        size: Size {
            width: convert_dimension(props.width),
            height: convert_dimension(props.height),
        },
        min_size: Size {
            width: convert_dimension(props.min_width),
            height: convert_dimension(props.min_height),
        },
        max_size: Size {
            width: convert_dimension(props.max_width),
            height: convert_dimension(props.max_height),
        },
        padding: taffy::Rect {
            top: convert_length_percent(props.padding.top),
            right: convert_length_percent(props.padding.right),
            bottom: convert_length_percent(props.padding.bottom),
            left: convert_length_percent(props.padding.left),
        },
        margin: taffy::Rect {
            top: convert_length_percent_auto(props.margin.top),
            right: convert_length_percent_auto(props.margin.right),
            bottom: convert_length_percent_auto(props.margin.bottom),
            left: convert_length_percent_auto(props.margin.left),
        },
        overflow: taffy::Point {
            x: match props.overflow {
                Overflow::Visible => taffy::Overflow::Visible,
                Overflow::Hidden => taffy::Overflow::Hidden,
                Overflow::Scroll => taffy::Overflow::Scroll,
            },
            y: match props.overflow {
                Overflow::Visible => taffy::Overflow::Visible,
                Overflow::Hidden => taffy::Overflow::Hidden,
                Overflow::Scroll => taffy::Overflow::Scroll,
            },
        },
        ..Default::default()
    }
}

fn convert_dimension(d: Dimension) -> taffy::Dimension {
    match d {
        Dimension::Auto => taffy::Dimension::Auto,
        Dimension::Px(v) => taffy::Dimension::Length(v),
        Dimension::Percent(v) => taffy::Dimension::Percent(v / 100.0),
    }
}

fn convert_length_percent(d: Dimension) -> LengthPercentage {
    match d {
        Dimension::Px(v) => LengthPercentage::Length(v),
        Dimension::Percent(v) => LengthPercentage::Percent(v / 100.0),
        Dimension::Auto => LengthPercentage::Length(0.0),
    }
}

fn convert_length_percent_auto(d: Dimension) -> LengthPercentageAuto {
    match d {
        Dimension::Auto => LengthPercentageAuto::Auto,
        Dimension::Px(v) => LengthPercentageAuto::Length(v),
        Dimension::Percent(v) => LengthPercentageAuto::Percent(v / 100.0),
    }
}

fn length(v: f32) -> LengthPercentage {
    LengthPercentage::Length(v)
}
