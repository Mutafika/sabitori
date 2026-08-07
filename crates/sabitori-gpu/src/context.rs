use std::sync::Arc;

/// Read-only GPU context passed to apps at setup time.
/// Contains everything needed to create custom pipelines, buffers, and bind groups.
pub struct GpuContext {
    pub device: Arc<wgpu::Device>,
    pub queue: Arc<wgpu::Queue>,
    pub surface_format: wgpu::TextureFormat,
    pub depth_format: wgpu::TextureFormat,
    pub surface_width: u32,
    pub surface_height: u32,
    pub scale_factor: f32,
}

/// Per-frame context passed to SceneApp::render_scene().
/// The app creates its own render pass using these resources.
pub struct SceneRenderContext<'a> {
    pub device: &'a wgpu::Device,
    pub queue: &'a wgpu::Queue,
    pub encoder: &'a mut wgpu::CommandEncoder,
    pub surface_view: &'a wgpu::TextureView,
    pub depth_view: &'a wgpu::TextureView,
    pub surface_format: wgpu::TextureFormat,
    pub depth_format: wgpu::TextureFormat,
    pub width: u32,
    pub height: u32,
    pub scale_factor: f32,
}
