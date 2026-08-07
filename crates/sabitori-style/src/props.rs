use sabitori_core::{Color, Corners, Point};
use serde::{Deserialize, Serialize};

/// CSS-like dimension value.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Dimension {
    Auto,
    Px(f32),
    Percent(f32),
}

impl Default for Dimension {
    fn default() -> Self {
        Self::Auto
    }
}

/// Display mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Display {
    #[default]
    Flex,
    Grid,
    None,
}

/// Flex direction.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlexDirection {
    #[default]
    Row,
    Column,
    RowReverse,
    ColumnReverse,
}

/// Flex wrap behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FlexWrap {
    #[default]
    NoWrap,
    Wrap,
    WrapReverse,
}

/// Alignment on the cross axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlignItems {
    #[default]
    Stretch,
    Start,
    End,
    Center,
}

/// Alignment on the main axis.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum JustifyContent {
    #[default]
    Start,
    End,
    Center,
    SpaceBetween,
    SpaceAround,
    SpaceEvenly,
}

/// Overflow behavior.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Overflow {
    #[default]
    Visible,
    Hidden,
    Scroll,
}

/// Position type.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Position {
    #[default]
    Relative,
    Absolute,
}

/// Edge values (padding, margin).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EdgeDimensions {
    pub top: Dimension,
    pub right: Dimension,
    pub bottom: Dimension,
    pub left: Dimension,
}

impl EdgeDimensions {
    pub fn all(v: Dimension) -> Self {
        Self { top: v, right: v, bottom: v, left: v }
    }

    pub fn px(v: f32) -> Self {
        Self::all(Dimension::Px(v))
    }

    pub fn axes(vertical: Dimension, horizontal: Dimension) -> Self {
        Self { top: vertical, right: horizontal, bottom: vertical, left: horizontal }
    }
}

/// Box shadow definition.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct BoxShadow {
    pub color: Color,
    pub offset: Point,
    pub blur: f32,
    pub spread: f32,
}

impl Default for BoxShadow {
    fn default() -> Self {
        Self {
            color: Color::TRANSPARENT,
            offset: Point::ZERO,
            blur: 0.0,
            spread: 0.0,
        }
    }
}

/// Fill type for backgrounds.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Fill {
    Solid(Color),
    LinearGradient {
        angle: f32,
        stops: Vec<(f32, Color)>,
    },
}

impl Default for Fill {
    fn default() -> Self {
        Self::Solid(Color::TRANSPARENT)
    }
}

/// Complete style properties for a UI element.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StyleProps {
    // Layout
    pub display: Display,
    pub position: Position,
    pub flex_direction: FlexDirection,
    pub flex_wrap: FlexWrap,
    pub align_items: AlignItems,
    pub justify_content: JustifyContent,
    pub flex_grow: f32,
    pub flex_shrink: f32,
    pub gap: f32,
    pub width: Dimension,
    pub height: Dimension,
    pub min_width: Dimension,
    pub max_width: Dimension,
    pub min_height: Dimension,
    pub max_height: Dimension,
    pub padding: EdgeDimensions,
    pub margin: EdgeDimensions,
    pub overflow: Overflow,

    // Visual
    pub background: Fill,
    pub corner_radius: Corners<f32>,
    pub border_color: Color,
    pub border_width: f32,
    pub shadow: Option<BoxShadow>,
    pub opacity: f32,

    // Text (inherited)
    pub color: Color,
    pub font_size: f32,
}

impl Default for StyleProps {
    fn default() -> Self {
        Self {
            display: Display::Flex,
            position: Position::Relative,
            flex_direction: FlexDirection::Row,
            flex_wrap: FlexWrap::NoWrap,
            align_items: AlignItems::Stretch,
            justify_content: JustifyContent::Start,
            flex_grow: 0.0,
            flex_shrink: 1.0,
            gap: 0.0,
            width: Dimension::Auto,
            height: Dimension::Auto,
            min_width: Dimension::Auto,
            max_width: Dimension::Auto,
            min_height: Dimension::Auto,
            max_height: Dimension::Auto,
            padding: EdgeDimensions::default(),
            margin: EdgeDimensions::default(),
            overflow: Overflow::Visible,
            background: Fill::Solid(Color::TRANSPARENT),
            corner_radius: Corners::all(0.0),
            border_color: Color::TRANSPARENT,
            border_width: 0.0,
            shadow: None,
            opacity: 1.0,
            color: Color::WHITE,
            font_size: 14.0,
        }
    }
}

/// Convenience trait for building styles.
pub trait DimensionExt {
    fn px(self) -> Dimension;
    fn pct(self) -> Dimension;
}

impl DimensionExt for f32 {
    fn px(self) -> Dimension { Dimension::Px(self) }
    fn pct(self) -> Dimension { Dimension::Percent(self) }
}

impl DimensionExt for i32 {
    fn px(self) -> Dimension { Dimension::Px(self as f32) }
    fn pct(self) -> Dimension { Dimension::Percent(self as f32) }
}
