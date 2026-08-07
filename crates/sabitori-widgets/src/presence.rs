use std::collections::HashMap;
use sabitori_anim::{Animated, Spring};
use sabitori_core::Element;

/// Tracks mount/unmount animations for elements with `animate_presence`.
///
/// When an element with `.animate_presence()` and `.id()` appears in the tree,
/// its opacity animates from 0 to 1 (mount). When it disappears, opacity animates
/// from 1 to 0 (unmount). The app can query `progress("id")` to keep rendering
/// exiting elements during the exit animation.
pub struct PresenceAnimator {
    /// Currently tracked elements: id -> state
    entries: HashMap<String, PresenceEntry>,
}

struct PresenceEntry {
    /// 0.0 = not present, 1.0 = fully present
    progress: Animated<f32>,
    /// Whether the element was in the current frame's tree
    present_this_frame: bool,
}

impl PresenceAnimator {
    pub fn new() -> Self {
        Self { entries: HashMap::new() }
    }

    /// Call after building the element tree. Scans for elements with animate_presence
    /// and tracks which IDs are present this frame.
    pub fn update_presence(&mut self, element: &Element) {
        // Mark all as not-present-this-frame
        for entry in self.entries.values_mut() {
            entry.present_this_frame = false;
        }
        // Walk tree, mark present ones
        self.scan_tree(element);
        // For newly absent elements, start exit animation
        for entry in self.entries.values_mut() {
            if !entry.present_this_frame {
                entry.progress.set_target(0.0);
            }
        }
        // Remove fully exited entries (progress near 0 and not present)
        self.entries.retain(|_, e| e.present_this_frame || e.progress.value() > 0.01);
    }

    fn scan_tree(&mut self, element: &Element) {
        if element.animate_presence {
            if let Some(ref id) = element.id {
                let entry = self.entries.entry(id.clone()).or_insert_with(|| {
                    PresenceEntry {
                        progress: Animated::new(0.0).with_spring(Spring {
                            stiffness: 400.0,
                            damping: 30.0,
                            mass: 1.0,
                        }),
                        present_this_frame: false,
                    }
                });
                entry.present_this_frame = true;
                entry.progress.set_target(1.0); // animate in
            }
        }
        for child in &element.children {
            self.scan_tree(child);
        }
    }

    /// Apply presence animation to elements: set opacity based on animation progress.
    pub fn apply(&self, element: &mut Element) {
        if element.animate_presence {
            if let Some(ref id) = element.id {
                if let Some(entry) = self.entries.get(id) {
                    let p = entry.progress.value();
                    element.style.opacity *= p;
                }
            }
        }
        for child in &mut element.children {
            self.apply(child);
        }
    }

    /// Get the animation progress for a specific element (0.0-1.0).
    /// Apps can use this to keep rendering exiting elements.
    pub fn progress(&self, id: &str) -> f32 {
        self.entries.get(id).map_or(0.0, |e| e.progress.value())
    }

    /// Check if an element is currently animating (entering or exiting).
    pub fn is_animating(&self, id: &str) -> bool {
        self.entries.get(id).map_or(false, |e| {
            let p = e.progress.value();
            let t = if e.present_this_frame { 1.0 } else { 0.0 };
            (p - t).abs() > 0.01
        })
    }

    /// Get all tracked element progress values as a map.
    pub fn all_progress(&self) -> HashMap<String, f32> {
        self.entries.iter().map(|(id, e)| (id.clone(), e.progress.value())).collect()
    }

    /// Tick all animations.
    pub fn tick(&mut self, dt: f32) {
        for entry in self.entries.values_mut() {
            entry.progress.tick(dt);
        }
    }

    /// True if any tracked element is mid-mount or mid-unmount.
    pub fn has_animations(&self) -> bool {
        self.entries.values().any(|e| {
            let p = e.progress.value();
            let t = if e.present_this_frame { 1.0 } else { 0.0 };
            e.progress.running || (p - t).abs() > 0.01
        })
    }
}
