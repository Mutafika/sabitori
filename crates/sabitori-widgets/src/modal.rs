use sabitori_anim::{Animated, Spring};
use sabitori_core::{Color, Element, Rect};
use sabitori_core::element::{div, Px, Role};

/// Style for modal dialog.
#[derive(Clone, Debug)]
pub struct ModalStyle {
    pub backdrop_color: Color,
    pub bg: Color,
    pub border_color: Color,
    pub corner_radius: f32,
    pub shadow_blur: f32,
    pub max_width: f32,
    pub max_height: f32,
    pub padding: f32,
}

impl ModalStyle {
    pub fn default_dark() -> Self {
        Self {
            backdrop_color: Color::new(0.0, 0.0, 0.0, 0.63),
            bg: Color::from_hex("#1e1e2e"),
            border_color: Color::from_hex("#3a3a55"),
            corner_radius: 12.0,
            shadow_blur: 32.0,
            max_width: 500.0,
            max_height: 400.0,
            padding: 24.0,
        }
    }
}

/// Modal dialog overlay.
pub struct Modal {
    pub visible: bool,
    pub title: String,
    pub style: ModalStyle,
    /// Viewport size for centering.
    pub viewport: Rect,
    /// Open/close animation (0=closed, 1=open).
    pub open_anim: Animated<f32>,
}

impl Modal {
    pub fn new(title: &str, style: ModalStyle) -> Self {
        Self {
            visible: false,
            title: title.to_string(),
            style,
            viewport: Rect::new(0.0, 0.0, 800.0, 600.0),
            open_anim: Animated::new(0.0).with_spring(Spring::snappy()),
        }
    }

    /// Open the modal.
    pub fn open(&mut self) {
        self.visible = true;
        self.open_anim.set_target(1.0);
    }

    /// Close the modal.
    pub fn close(&mut self) {
        self.visible = false;
        self.open_anim.set_target(0.0);
    }

    /// Toggle visibility.
    pub fn toggle(&mut self) {
        if self.visible {
            self.close();
        } else {
            self.open();
        }
    }

    /// Set viewport size (for centering).
    pub fn set_viewport(&mut self, width: f32, height: f32) {
        self.viewport = Rect::new(0.0, 0.0, width, height);
    }

    /// The backdrop rect (full viewport).
    pub fn backdrop_rect(&self) -> Rect {
        self.viewport
    }

    /// Backdrop opacity (animated).
    pub fn backdrop_opacity(&self) -> f32 {
        self.open_anim.value()
    }

    /// The modal dialog rect (centered, animated scale).
    pub fn dialog_rect(&self) -> Rect {
        let scale = self.open_anim.value();
        let w = self.style.max_width * scale;
        let h = self.style.max_height * scale;
        let x = (self.viewport.size.width - w) / 2.0;
        let y = (self.viewport.size.height - h) / 2.0;
        Rect::new(x, y, w, h)
    }

    /// The content area inside the dialog (with padding).
    pub fn content_rect(&self) -> Rect {
        let d = self.dialog_rect();
        let p = self.style.padding;
        Rect::new(
            d.origin.x + p,
            d.origin.y + p,
            d.size.width - p * 2.0,
            d.size.height - p * 2.0,
        )
    }

    /// Whether animation is complete (for cleanup).
    pub fn is_fully_closed(&self) -> bool {
        !self.visible && self.open_anim.value() < 0.01
    }

    /// Whether the modal is open (or animating to open).
    pub fn is_open(&self) -> bool {
        self.visible
    }

    /// Current animation progress (0.0 = fully closed, 1.0 = fully open).
    pub fn progress(&self) -> f32 {
        self.open_anim.value()
    }

    /// Tick animations.
    pub fn tick(&mut self, dt: f32) {
        self.open_anim.tick(dt);
    }

    /// Build a complete overlay Element for this modal.
    ///
    /// Returns `None` if the modal is fully closed (not visible and animation done).
    ///
    /// * `viewport_w`, `viewport_h` — viewport dimensions for backdrop sizing.
    /// * `backdrop_id` — element ID for the backdrop (for click-to-dismiss).
    /// * `dialog_w` — desired width of the dialog.
    /// * `bg` — dialog background color.
    /// * `border` — dialog border color.
    /// * `content` — child elements to place inside the dialog.
    pub fn to_overlay(
        &self,
        viewport_w: f32,
        viewport_h: f32,
        backdrop_id: &str,
        dialog_w: f32,
        bg: Color,
        border: Color,
        content: Vec<Element>,
    ) -> Option<Element> {
        if !self.is_open() && self.progress() <= 0.01 {
            return None;
        }

        let progress = self.progress();
        let backdrop_alpha = progress * 0.63;
        let padding = self.style.padding;

        // Center the dialog horizontally and vertically
        let left = (viewport_w - dialog_w) / 2.0;
        let dialog_h = self.style.max_height;
        let top = (viewport_h - dialog_h) / 2.0;

        // Dialog panel
        let dialog = div()
            .role(Role::Dialog)
            .label(&self.title)
            .pos(left, top)
            .w(Px(dialog_w))
            .h(Px(dialog_h))
            .bg(bg)
            .border(1.0, border)
            .rounded_px(self.style.corner_radius)
            .shadow_md(Color::new(0.0, 0.0, 0.0, 0.5))
            .opacity(progress)
            .p(Px(padding))
            .flex_col()
            .overflow_hidden()
            .children(content);

        // Backdrop + dialog
        let overlay = div()
            .id(backdrop_id)
            .w(Px(viewport_w))
            .h(Px(viewport_h))
            .pos(0.0, 0.0)
            .bg(Color::new(0.0, 0.0, 0.0, backdrop_alpha))
            .overlay()
            .child(dialog);

        Some(overlay)
    }
}
