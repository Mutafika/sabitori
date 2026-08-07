use sabitori_core::{Color, Corners, Point, Rect};
use sabitori_anim::{Animated, Spring};

#[derive(Clone, Debug)]
pub struct CardStyle {
    pub fill: Color,
    pub fill_hover: Color,
    pub border_color: Color,
    pub border_hover: Color,
    pub corner_radius: f32,
    pub shadow_blur: f32,
    pub shadow_hover_blur: f32,
}

impl CardStyle {
    pub fn default_dark() -> Self {
        Self {
            fill: Color::from_hex("#22223a"),
            fill_hover: Color::from_hex("#2a2a48"),
            border_color: Color::from_hex("#3a3a55"),
            border_hover: Color::from_hex("#6c63ff80"),
            corner_radius: 12.0,
            shadow_blur: 16.0,
            shadow_hover_blur: 24.0,
        }
    }
}

pub struct Card {
    pub bounds: Rect,
    pub style: CardStyle,
    pub fill_anim: Animated<Color>,
    pub border_anim: Animated<Color>,
    pub shadow_anim: Animated<f32>,
    pub hovered: bool,
}

impl Card {
    pub fn new(x: f32, y: f32, width: f32, height: f32, style: CardStyle) -> Self {
        let fill = style.fill;
        let border = style.border_color;
        Self {
            bounds: Rect::new(x, y, width, height),
            style,
            fill_anim: Animated::new(fill).with_spring(Spring::snappy()),
            border_anim: Animated::new(border).with_spring(Spring::snappy()),
            shadow_anim: Animated::new(0.0).with_spring(Spring::snappy()),
            hovered: false,
        }
    }

    pub fn set_hover(&mut self, hovered: bool) {
        if self.hovered == hovered {
            return;
        }
        self.hovered = hovered;
        if hovered {
            self.fill_anim.set_target(self.style.fill_hover);
            self.border_anim.set_target(self.style.border_hover);
            self.shadow_anim.set_target(1.0);
        } else {
            self.fill_anim.set_target(self.style.fill);
            self.border_anim.set_target(self.style.border_color);
            self.shadow_anim.set_target(0.0);
        }
    }

    pub fn tick(&mut self, dt: f32) {
        self.fill_anim.tick(dt);
        self.border_anim.tick(dt);
        self.shadow_anim.tick(dt);
    }

    pub fn hit_test(&self, point: Point) -> bool {
        self.bounds.contains(point)
    }
}
