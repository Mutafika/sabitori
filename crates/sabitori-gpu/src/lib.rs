mod camera;
mod context;
mod image_renderer;
mod instance;
mod line_renderer;
mod renderer;
mod ring_renderer;
mod ui_overlay;

/// Re-exported so downstream crates can name wgpu types in their own signatures
/// without taking a second `wgpu` dependency (which could drift to a different
/// version and silently stop unifying with ours).
pub use wgpu;

pub use camera::OrbitCamera;
pub use context::{GpuContext, SceneRenderContext};
pub use image_renderer::{ImageInstance, ImageRenderer};
pub use instance::{LineInstance, RectInstance, RingInstance};
pub use line_renderer::LineRenderer;
pub use renderer::{GpuRenderer, RenderPhase};
pub use ring_renderer::RingRenderer;
pub use ui_overlay::UiOverlayRenderer;
