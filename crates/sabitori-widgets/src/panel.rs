use sabitori_anim::{Animated, Spring};

#[derive(Clone, Copy, PartialEq)]
pub enum PanelSide {
    Left,
    Right,
}

pub struct Panel {
    pub side: PanelSide,
    pub width: f32,
    visible: bool,
    slide: Animated<f32>, // 0.0 = hidden, 1.0 = fully shown
}

impl Panel {
    pub fn new(side: PanelSide, width: f32) -> Self {
        Self {
            side,
            width,
            visible: false,
            slide: Animated::new(0.0).with_spring(Spring::snappy()),
        }
    }

    pub fn open(&mut self) {
        self.visible = true;
        self.slide.set_target(1.0);
    }

    pub fn close(&mut self) {
        self.visible = false;
        self.slide.set_target(0.0);
    }

    pub fn toggle(&mut self) {
        if self.visible {
            self.close();
        } else {
            self.open();
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible || self.slide.value() > 0.01
    }

    pub fn is_open(&self) -> bool {
        self.visible
    }

    /// Current visual width (animated).
    pub fn current_width(&self) -> f32 {
        self.width * self.slide.value()
    }

    pub fn tick(&mut self, dt: f32) {
        self.slide.tick(dt);
    }

    /// Build the panel Element. Returns None if fully hidden.
    /// The panel is a fixed-width div meant to be placed in a flex-row
    /// alongside the main content. The width animates via spring.
    ///
    /// `content` is the panel's inner content elements.
    /// The caller should also render a 1px divider next to it.
    pub fn to_element(
        &self,
        bg: sabitori_core::Color,
        _border: sabitori_core::Color,
        content: Vec<sabitori_core::Element>,
    ) -> Option<sabitori_core::Element> {
        if !self.is_visible() {
            return None;
        }

        use sabitori_core::element::{div, Px};

        let w = self.current_width();
        if w < 1.0 {
            return None;
        }

        Some(
            div()
                .w(Px(w))
                .shrink(0.0)
                .bg(bg)
                .overflow_hidden()
                .flex_col()
                .children(content),
        )
    }

    /// Build panel + divider as a pair of elements to insert in a flex-row.
    /// Returns empty vec if not visible.
    pub fn to_elements_with_divider(
        &self,
        bg: sabitori_core::Color,
        border: sabitori_core::Color,
        content: Vec<sabitori_core::Element>,
    ) -> Vec<sabitori_core::Element> {
        use sabitori_core::element::{div, Px};

        if !self.is_visible() {
            return vec![];
        }

        let w = self.current_width();
        if w < 1.0 {
            return vec![];
        }

        let divider = div().w(Px(1.0)).shrink(0.0).bg(border);

        let panel = div()
            .w(Px(w))
            .shrink(0.0)
            .bg(bg)
            .overflow_hidden()
            .flex_col()
            .children(content);

        match self.side {
            PanelSide::Left => vec![panel, divider],
            PanelSide::Right => vec![divider, panel],
        }
    }
}
