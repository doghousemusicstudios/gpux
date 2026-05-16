use crate::abi::types::*;
#[cfg(target_os = "android")]
use crate::abi::enums::texture_format_from_u32;
#[cfg(target_os = "android")]
use crate::runtime::handle::*;
#[cfg(target_os = "android")]
use crate::runtime::state::*;
use crate::{ffi_catch, set_error};

/// Import an AHardwareBuffer as a `wgpu::Texture` handle.
///
/// Returns 0 on failure. The caller can read `wgpu_get_last_error()` for a
/// human-readable reason.
///
/// Requirements (Android):
/// - `device` was created on the Vulkan backend with
///   `VK_ANDROID_external_memory_android_hardware_buffer` enabled.
/// - `ahb` is a valid `AHardwareBuffer*` whose desc matches `width`/`height`
///   and whose format maps to the requested `format`.
/// - The AHB usage must include `AHARDWAREBUFFER_USAGE_GPU_SAMPLED_IMAGE`.
///
/// The native side calls `AHardwareBuffer_acquire` so the caller can release
/// its own reference immediately after this returns. The acquired reference
/// is dropped when the returned `wgpu::Texture` handle is released.
#[cfg(target_os = "android")]
#[export_name = "wgpun_DeviceImportAHardwareBuffer"]
pub extern "C" fn wgpun_DeviceImportAHardwareBuffer(
    device: WGPUDevice,
    ahb: *mut std::ffi::c_void,
    width: u32,
    height: u32,
    format: u32,
) -> WGPUTexture {
    if device == 0 || ahb.is_null() || width == 0 || height == 0 {
        set_error("wgpun_DeviceImportAHardwareBuffer: invalid argument");
        return 0;
    }
    ffi_catch!(0, {
        let entry = unsafe { deref_handle::<DeviceEntry>(device) };
        let wgpu_format = texture_format_from_u32(format);
        match super::import::import_ahardware_buffer_to_wgpu(
            &entry.device,
            ahb,
            wgpu_format,
            width,
            height,
        ) {
            Ok(texture) => into_handle(texture),
            Err(error) => {
                set_error(error);
                0
            }
        }
    })
}

#[cfg(not(target_os = "android"))]
#[export_name = "wgpun_DeviceImportAHardwareBuffer"]
pub extern "C" fn wgpun_DeviceImportAHardwareBuffer(
    _device: WGPUDevice,
    _ahb: *mut std::ffi::c_void,
    _width: u32,
    _height: u32,
    _format: u32,
) -> WGPUTexture {
    ffi_catch!(0, {
        set_error("AHardwareBuffer import is only supported on Android");
        0
    })
}
