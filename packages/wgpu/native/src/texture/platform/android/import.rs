//! Vulkan-backed AHardwareBuffer import.
//!
//! Milestone 1 placeholder. Real impl lands in Milestone 2.

#![cfg(target_os = "android")]

/// Import an AHB into a `wgpu::Texture`. Placeholder until M2.
pub(crate) fn import_ahardware_buffer_to_wgpu(
    _device: &wgpu::Device,
    _ahb: *mut std::ffi::c_void,
    _format: wgpu::TextureFormat,
    _width: u32,
    _height: u32,
) -> Result<wgpu::Texture, String> {
    Err("AHardwareBuffer import not yet implemented (milestone 1 scaffold)".to_string())
}
