use sabitori_core::{Color, Corners, Point, Rect};
use sabitori_anim::{Animated, Spring};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonVariant {
    Primary,
    Secondary,
    Ghost,
}

#[derive(Clone, Debug)]
pub struct ButtonStyle {
    pub fill: Color,
    pub fill_hover: Color,
    pub fill_active: Color,
    pub text_color: Color,
    pub border_color: Color,
    pub corner_radius: f32,
    pub height: f32,
    pub padding_h: f32,
}

impl ButtonStyle {
    pub fn primary(accent: Color) -> Self {
        Self {
            fill: accent,
            fill_hover: accent.lighten(0.15),
            fill_active: accent.darken(0.1),
            text_color: Color::WHITE,
            border_color: Color::TRANSPARENT,
            corner_radius: 8.0,
            height: 36.0,
            padding_h: 16.0,
        }
    }

    pub fn secondary(accent: Color) -> Self {
        Self {
            fill: Color::TRANSPARENT,
            fill_hover: accent.with_alpha(0.1),
            fill_active: accent.with_alpha(0.2),
            text_color: accent,
            border_color: accent.with_alpha(0.5),
            corner_radius: 8.0,
            height: 36.0,
            padding_h: 16.0,
        }
    }

    pub fn ghost() -> Self {
        Self {
            fill: Color::TRANSPARENT,
            fill_hover: Color::from_hex("#ffffff10"),
            fill_active: Color::from_hex("#ffffff20"),
            text_color: Color::from_hex("#e8e8f0"),
            border_color: Color::TRANSPARENT,
            corner_radius: 6.0,
            height: 32.0,
            padding_h: 12.0,
        }
    }
}

pub struct Button {
    pub bounds: Rect,
    pub label: String,
    pub style: ButtonStyle,
    pub fill_anim: Animated<Color>,
    pub shadow_anim: Animated<f32>,
    pub hovered: bool,
    pub pressed: bool,
}

impl Button {
    pub fn new(x: f32, y: f32, width: f32, label: impl Into<String>, style: ButtonStyle) -> Self {
        let fill = style.fill;
        Self {
            bounds: Rect::new(x, y, width, style.height),
            label: label.into(),
            style,
            fill_anim: Animated::new(fill).with_spring(Spring::snappy()),
            shadow_anim: Animated::new(0.0).with_spring(Spring::snappy()),
            hovered: false,
            pressed: false,
        }
    }

    pub fn set_hover(&mut self, hovered: bool) {
        if self.hovered == hovered {
            return;
        }
        self.hovered = hovered;
        self.update_visual();
    }

    pub fn set_pressed(&mut self, pressed: bool) {
        if self.pressed == pressed {
            return;
        }
        self.pressed = pressed;
        self.update_visual();
    }

    fn update_visual(&mut self) {
        if self.pressed {
            self.fill_anim.set_target(self.style.fill_active);
            self.shadow_anim.set_target(0.0);
        } else if self.hovered {
            self.fill_anim.set_target(self.style.fill_hover);
            self.shadow_anim.set_target(1.0);
        } else {
            self.fill_anim.set_target(self.style.fill);
            self.shadow_anim.set_target(0.0);
        }
    }

    pub fn tick(&mut self, dt: f32) {
        self.fill_anim.tick(dt);
        self.shadow_anim.tick(dt);
    }

    pub fn hit_test(&self, point: Point) -> bool {
        self.bounds.contains(point)
    }
}
