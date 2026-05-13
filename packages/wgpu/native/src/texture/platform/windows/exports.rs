use crate::abi::types::*;
#[cfg(target_os = "windows")]
use crate::ffi_catch;
use crate::set_error;

#[cfg(target_os = "windows")]
#[export_name = "wgpun_DeviceImportDxgiSharedTexture"]
pub extern "C" fn wgpuDeviceImportDxgiSharedTexture(
    _device: WGPUDevice,
    _shared_handle: u64,
    _owns_handle: u8,
    _width: u32,
    _height: u32,
    _format: u32,
    _usage: u32,
    _plane_count: u32,
    _color_space: u32,
    _keyed_mutex_enabled: u8,
    _keyed_mutex_acquire_key: u64,
    _keyed_mutex_release_key: u64,
    _keyed_mutex_timeout_ms: i64,
    _producer_adapter_luid_low: u32,
    _producer_adapter_luid_high: i32,
) -> WGPUTexture {
    ffi_catch!(0, {
        // TODO: Open the NT shared handle with the active D3D backend and wrap
        // it as an external WGPU texture. Until then, never report success.
        set_error("DXGI shared texture import is not implemented yet");
        0
    })
}

#[cfg(not(target_os = "windows"))]
#[export_name = "wgpun_DeviceImportDxgiSharedTexture"]
pub extern "C" fn wgpuDeviceImportDxgiSharedTexture(
    _device: WGPUDevice,
    _shared_handle: u64,
    _owns_handle: u8,
    _width: u32,
    _height: u32,
    _format: u32,
    _usage: u32,
    _plane_count: u32,
    _color_space: u32,
    _keyed_mutex_enabled: u8,
    _keyed_mutex_acquire_key: u64,
    _keyed_mutex_release_key: u64,
    _keyed_mutex_timeout_ms: i64,
    _producer_adapter_luid_low: u32,
    _producer_adapter_luid_high: i32,
) -> WGPUTexture {
    set_error("DXGI shared texture import is only supported on Windows");
    0
}
