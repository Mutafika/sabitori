use sabitori_anim::{Animated, Spring};
use sabitori_core::{Color, Element};
use sabitori_core::element::{div, text, Px};

/// The kind/severity of a toast notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Success,
    Warning,
    Error,
}

impl ToastKind {
    /// Return an icon character for the toast kind.
    fn icon(self) -> &'static str {
        match self {
            ToastKind::Info => "i",
            ToastKind::Success => "v",
            ToastKind::Warning => "!",
            ToastKind::Error => "x",
        }
    }
}

/// A single toast notification.
struct Toast {
    #[allow(dead_code)]
    id: u64,
    message: String,
    kind: ToastKind,
    elapsed: f32,
    duration: f32,
    opacity: Animated<f32>,
}

/// Manages a stack of toast notifications.
pub struct ToastManager {
    toasts: Vec<Toast>,
    next_id: u64,
}

impl ToastManager {
    pub fn new() -> Self {
        Self {
            toasts: Vec::new(),
            next_id: 0,
        }
    }

    /// Show a toast with the default duration (3 seconds).
    pub fn show(&mut self, message: impl Into<String>, kind: ToastKind) {
        self.show_for(message, kind, 3.0);
    }

    /// Show a toast with a custom duration in seconds.
    pub fn show_for(&mut self, message: impl Into<String>, kind: ToastKind, seconds: f32) {
        let id = self.next_id;
        self.next_id += 1;

        let mut opacity = Animated::new(0.0).with_spring(Spring::snappy());
        opacity.set_target(1.0);

        self.toasts.push(Toast {
            id,
            message: message.into(),
            kind,
            elapsed: 0.0,
            duration: seconds,
            opacity,
        });
    }

    /// Advance timers. Fades out expiring toasts and removes fully faded ones.
    pub fn tick(&mut self, dt: f32) {
        for toast in &mut self.toasts {
            toast.elapsed += dt;
            toast.opacity.tick(dt);

            // Start fading out when duration is exceeded
            if toast.elapsed >= toast.duration && toast.opacity.value() > 0.01 {
                toast.opacity.set_target(0.0);
            }
        }

        // Remove fully faded-out toasts
        self.toasts.retain(|t| {
            !(t.elapsed >= t.duration && t.opacity.value() <= 0.01 && !t.opacity.running)
        });
    }

    /// Whether there are any toasts currently showing (or animating).
    pub fn has_toasts(&self) -> bool {
        !self.toasts.is_empty()
    }

    /// Build an overlay Element with the toast stack at bottom center.
    ///
    /// Returns `None` if there are no toasts.
    ///
    /// * `viewport_w`, `viewport_h` — viewport dimensions for positioning.
    /// * `bg` — toast background color.
    /// * `text_color` — toast text color.
    /// * `border` — toast border color.
    pub fn to_overlay(
        &self,
        viewport_w: f32,
        viewport_h: f32,
        bg: Color,
        text_color: Color,
        border: Color,
    ) -> Option<Element> {
        if self.toasts.is_empty() {
            return None;
        }

        let toast_w: f32 = 360.0;
        let toast_h: f32 = 44.0;
        let gap: f32 = 8.0;
        let bottom_margin: f32 = 32.0;

        // Build toast elements from bottom (newest) to top (oldest)
        let total_height = self.toasts.len() as f32 * (toast_h + gap) - gap;
        let stack_top = viewport_h - bottom_margin - total_height;
        let stack_left = (viewport_w - toast_w) / 2.0;

        let mut toast_elements: Vec<Element> = Vec::new();

        for (i, toast) in self.toasts.iter().enumerate() {
            let y_offset = i as f32 * (toast_h + gap);
            let opacity = toast.opacity.value();

            // Icon badge color based on kind
            let icon_bg = match toast.kind {
                ToastKind::Info => Color::from_hex("#4a90d9"),
                ToastKind::Success => Color::from_hex("#4caf50"),
                ToastKind::Warning => Color::from_hex("#ff9800"),
                ToastKind::Error => Color::from_hex("#f44336"),
            };

            let icon_el = div()
                .w(Px(22.0))
                .h(Px(22.0))
                .bg(icon_bg)
                .rounded_px(11.0)
                .items_center()
                .justify_center()
                .child(
                    text(toast.kind.icon())
                        .font_size(12.0)
                        .bold()
                        .color(Color::WHITE)
                );

            let msg_el = text(&toast.message)
                .font_size(13.0)
                .color(text_color);

            let pill = div()
                .pos(stack_left, stack_top + y_offset)
                .w(Px(toast_w))
                .h(Px(toast_h))
                .bg(bg)
                .border(1.0, border)
                .rounded_px(22.0)
                .shadow_sm(Color::new(0.0, 0.0, 0.0, 0.3))
                .opacity(opacity)
                .px_pad(Px(14.0))
                .flex_row()
                .items_center()
                .gap(10.0)
                .child(icon_el)
                .child(msg_el);

            toast_elements.push(pill);
        }

        // Transparent container that doesn't block clicks
        let overlay = div()
            .w(Px(viewport_w))
            .h(Px(viewport_h))
            .pos(0.0, 0.0)
            .overlay()
            .children(toast_elements);

        Some(overlay)
    }
}

impl Default for ToastManager {
    fn default() -> Self {
        Self::new()
    }
}
