//! `StyleProps` (YAML テーマ / retained な `NodeTree` 用の style 記述) が使う型。
//!
//! レイアウトの基本型 (`Dimension` / `Overflow` / `AlignItems` …) は
//! **`sabitori-core::element` の定義をそのまま使う**。 かつてはこのファイルが
//! 同じ名前の型を 9 個**別々に**定義していて、 ファサード越しに import した値が
//! `Element` のビルダーに渡らなかった (issue #24):
//!
//! ```text
//! error: expected `sabitori::element::Overflow`, found `sabitori::Overflow`
//! ```
//!
//! 構造も derive もほぼ同一で、 分けている理由が無かったので core に寄せた。

use sabitori_core::{Color, Corners, Point};
use serde::{Deserialize, Serialize};

// レイアウト基本型は core が正。 ここは再輸出だけ。
pub use sabitori_core::element::{
    AlignItems, BoxShadow, Dimension, DimensionExt, EdgeDimensions, FlexDirection, FlexWrap,
    JustifyContent, Overflow, Position,
};

/// Display mode.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Display {
    #[default]
    Flex,
    Grid,
    None,
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

