use std::collections::HashMap;
use sabitori_anim::{Animated, Spring};
use sabitori_core::Color;
use sabitori_core::element::{Element, TransitionKind};

/// Tracks animated style properties per element ID.
/// Elements with `transitions` get smooth spring/easing-based interpolation
/// between normal and hover states.
pub struct StyleAnimator {
    states: HashMap<String, AnimatedStyle>,
}

struct AnimatedStyle {
    bg: AnimatedColor,
    border_color: AnimatedColor,
    color: AnimatedColor,
    opacity: Animated<f32>,
    border_width: Animated<f32>,
    font_size: Animated<f32>,
}

struct AnimatedColor {
    r: Animated<f32>,
    g: Animated<f32>,
    b: Animated<f32>,
    a: Animated<f32>,
}

impl AnimatedColor {
    fn new(color: Color, spring: Spring) -> Self {
        Self {
            r: Animated::new(color.r).with_spring(spring),
            g: Animated::new(color.g).with_spring(spring),
            b: Animated::new(color.b).with_spring(spring),
            a: Animated::new(color.a).with_spring(spring),
        }
    }

    fn set_target(&mut self, color: Color) {
        self.r.set_target(color.r);
        self.g.set_target(color.g);
        self.b.set_target(color.b);
        self.a.set_target(color.a);
    }

    fn value(&self) -> Color {
        Color::new(
            self.r.value(),
            self.g.value(),
            self.b.value(),
            self.a.value(),
        )
    }

    fn tick(&mut self, dt: f32) {
        self.r.tick(dt);
        self.g.tick(dt);
        self.b.tick(dt);
        self.a.tick(dt);
    }

    fn is_running(&self) -> bool {
        self.r.running || self.g.running || self.b.running || self.a.running
    }
}

impl AnimatedStyle {
    fn is_running(&self) -> bool {
        self.bg.is_running()
            || self.border_color.is_running()
            || self.color.is_running()
            || self.opacity.running
            || self.border_width.running
            || self.font_size.running
    }
}

/// Extract a spring from the element's transition declarations.
fn spring_from_transitions(element: &Element) -> Spring {
    for t in &element.transitions {
        match t.kind {
            TransitionKind::Spring { stiffness, damping } => {
                return Spring { stiffness, damping, mass: 1.0 };
            }
            _ => {}
        }
    }
    // Default spring for easing-based transitions
    Spring { stiffness: 300.0, damping: 25.0, mass: 1.0 }
}

impl StyleAnimator {
    pub fn new() -> Self {
        Self { states: HashMap::new() }
    }

    /// Update animation targets based on the current hover / press state.
    /// Walk the element tree, and for each element with transitions, set the
    /// animated values to the active style, the hover style, or the base style.
    ///
    /// Press wins over hover — the same precedence `NodeStyle::effective_style`
    /// uses. Without `pressed_id` an element with transitions could never show
    /// its `active_style` at all, which is how `button()` (transitions by
    /// default) ended up with no press feedback.
    pub fn update(
        &mut self,
        element: &Element,
        hovered_id: &Option<String>,
        pressed_id: &Option<String>,
    ) {
        self.update_recursive(element, hovered_id, pressed_id);
    }

    fn update_recursive(
        &mut self,
        element: &Element,
        hovered_id: &Option<String>,
        pressed_id: &Option<String>,
    ) {
        if let Some(ref id) = element.id {
            if !element.transitions.is_empty() {
                let is_hovered = hovered_id.as_deref() == Some(id.as_str());
                let is_pressed = pressed_id.as_deref() == Some(id.as_str());
                let spring = spring_from_transitions(element);

                let entry = self.states.entry(id.clone()).or_insert_with(|| {
                    AnimatedStyle {
                        bg: AnimatedColor::new(element.style.background, spring),
                        border_color: AnimatedColor::new(element.style.border_color, spring),
                        color: AnimatedColor::new(element.style.color, spring),
                        opacity: Animated::new(element.style.opacity).with_spring(spring),
                        border_width: Animated::new(element.style.border_width).with_spring(spring),
                        font_size: Animated::new(element.style.font_size).with_spring(spring),
                    }
                });

                // 押下 → hover → 素、の順に効く方を選ぶ。押下中でも active_style が
                // 無ければ hover へ落ちるので、hover だけ書いた要素の挙動は不変。
                let state = if is_pressed { element.active_style.as_ref() } else { None }
                    .or(if is_hovered { element.hover_style.as_ref() } else { None });

                if let Some(st) = state {
                    // Set targets to the state's values (or base if not overridden)
                    entry.bg.set_target(st.background.unwrap_or(element.style.background));
                    entry.border_color.set_target(st.border_color.unwrap_or(element.style.border_color));
                    entry.color.set_target(st.color.unwrap_or(element.style.color));
                    entry.opacity.set_target(st.opacity.unwrap_or(element.style.opacity));
                    entry.border_width.set_target(st.border_width.unwrap_or(element.style.border_width));
                    entry.font_size.set_target(st.font_size.unwrap_or(element.style.font_size));
                } else {
                    // Return to base style
                    entry.bg.set_target(element.style.background);
                    entry.border_color.set_target(element.style.border_color);
                    entry.color.set_target(element.style.color);
                    entry.opacity.set_target(element.style.opacity);
                    entry.border_width.set_target(element.style.border_width);
                    entry.font_size.set_target(element.style.font_size);
                }
            }
        }
        for child in &element.children {
            self.update_recursive(child, hovered_id, pressed_id);
        }
    }

    /// Apply animated values back onto elements, overwriting their style fields.
    pub fn apply(&self, element: &mut Element) {
        if let Some(ref id) = element.id {
            if let Some(state) = self.states.get(id) {
                element.style.background = state.bg.value();
                element.style.border_color = state.border_color.value();
                element.style.color = state.color.value();
                element.style.opacity = state.opacity.value();
                element.style.border_width = state.border_width.value();
                element.style.font_size = state.font_size.value();
            }
        }
        for child in &mut element.children {
            self.apply(child);
        }
    }

    /// Tick all animations forward by `dt` seconds.
    pub fn tick(&mut self, dt: f32) {
        for state in self.states.values_mut() {
            state.bg.tick(dt);
            state.border_color.tick(dt);
            state.color.tick(dt);
            state.opacity.tick(dt);
            state.border_width.tick(dt);
            state.font_size.tick(dt);
        }
    }

    /// Returns true if any animation is still running.
    pub fn is_animating(&self) -> bool {
        self.states.values().any(|s| s.is_running())
    }
}
