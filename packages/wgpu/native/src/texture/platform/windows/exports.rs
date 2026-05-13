use crate::abi::types::*;
#[cfg(target_os = "windows")]
use crate::ffi_catch;
#[cfg(target_os = "windows")]
use crate::runtime::handle::*;
#[cfg(target_os = "windows")]
use crate::runtime::state::*;
use crate::set_error;

#[cfg(target_os = "windows")]
use std::ffi::c_void;
#[cfg(target_os = "windows")]
use windows::core::PCWSTR;
#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::{CloseHandle, GENERIC_ALL, HANDLE},
    Graphics::{
        Direct3D12::{
            ID3D12Resource, D3D12_CPU_PAGE_PROPERTY_UNKNOWN, D3D12_HEAP_FLAG_SHARED,
            D3D12_HEAP_PROPERTIES, D3D12_HEAP_TYPE_DEFAULT, D3D12_MEMORY_POOL_UNKNOWN,
            D3D12_RESOURCE_DESC, D3D12_RESOURCE_DIMENSION_TEXTURE2D,
            D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET, D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS,
            D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS, D3D12_RESOURCE_STATE_COMMON,
            D3D12_TEXTURE_LAYOUT_UNKNOWN,
        },
        Dxgi::Common::{
            DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
            DXGI_SAMPLE_DESC,
        },
    },
};

#[cfg(target_os = "windows")]
fn close_owned_handle(handle: HANDLE, owns_handle: u8) {
    if owns_handle != 0 {
        let _ = unsafe { CloseHandle(handle) };
    }
}

#[cfg(target_os = "windows")]
fn import_format(format: u32) -> Option<(wgpu::TextureFormat, DXGI_FORMAT)> {
    match format {
        22 => Some((wgpu::TextureFormat::Bgra8Unorm, DXGI_FORMAT_B8G8R8A8_UNORM)),
        23 => Some((
            wgpu::TextureFormat::Bgra8UnormSrgb,
            DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
        )),
        _ => None,
    }
}

#[cfg(target_os = "windows")]
fn allowed_usage_for_resource(
    flags: windows::Win32::Graphics::Direct3D12::D3D12_RESOURCE_FLAGS,
) -> wgpu::TextureUsages {
    let mut usage = wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::COPY_DST
        | wgpu::TextureUsages::TEXTURE_BINDING;
    if flags.contains(D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS) {
        usage |= wgpu::TextureUsages::STORAGE_BINDING;
    }
    if flags.contains(D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET) {
        usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
    }
    usage
}

#[cfg(target_os = "windows")]
fn d3d12_synthetic_texture_desc(width: u32, height: u32) -> D3D12_RESOURCE_DESC {
    D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_TEXTURE2D,
        Alignment: 0,
        Width: width as u64,
        Height: height,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_UNKNOWN,
        Flags: D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS,
    }
}

#[cfg(target_os = "windows")]
fn run_d3d12_dxgi_shared_texture_synthetic_proof(device: WGPUDevice) -> Result<(), String> {
    if device == 0 {
        return Err("device must not be 0".to_string());
    }

    let entry = unsafe { deref_handle::<DeviceEntry>(device) };
    let hal_device = match unsafe { entry.device.as_hal::<wgpu::hal::api::Dx12>() } {
        Some(hal_device) => hal_device,
        None => {
            return Err(
                "DXGI D3D12 shared texture synthetic proof requires the D3D12 backend".to_string(),
            );
        }
    };
    let raw_device = hal_device.raw_device();
    let heap_properties = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_DEFAULT,
        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
        CreationNodeMask: 1,
        VisibleNodeMask: 1,
    };
    let texture_desc = d3d12_synthetic_texture_desc(64, 64);
    let mut resource: Option<ID3D12Resource> = None;
    unsafe {
        raw_device.CreateCommittedResource(
            &heap_properties,
            D3D12_HEAP_FLAG_SHARED,
            &texture_desc,
            D3D12_RESOURCE_STATE_COMMON,
            None,
            &mut resource,
        )
    }
    .map_err(|error| format!("ID3D12Device::CreateCommittedResource failed: {error}"))?;

    let resource = resource
        .ok_or_else(|| "ID3D12Device::CreateCommittedResource returned no texture".to_string())?;
    let shared_handle =
        unsafe { raw_device.CreateSharedHandle(&resource, None, GENERIC_ALL.0, PCWSTR::null()) }
            .map_err(|error| format!("ID3D12Device::CreateSharedHandle failed: {error}"))?;

    let luid = unsafe { raw_device.GetAdapterLuid() };
    let texture = import_dxgi_shared_texture(
        device,
        shared_handle.0 as usize as u64,
        1,
        64,
        64,
        22,
        (wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC).bits(),
        1,
        0,
        luid.LowPart,
        luid.HighPart,
    )?;
    drop(texture);
    Ok(())
}

#[cfg(target_os = "windows")]
#[allow(clippy::too_many_arguments)]
fn import_dxgi_shared_texture(
    device: WGPUDevice,
    shared_handle: u64,
    owns_handle: u8,
    width: u32,
    height: u32,
    format: u32,
    usage: u32,
    plane_count: u32,
    keyed_mutex_enabled: u8,
    producer_adapter_luid_low: u32,
    producer_adapter_luid_high: i32,
) -> Result<wgpu::Texture, String> {
    if shared_handle == 0 {
        return Err("sharedHandle must not be 0".to_string());
    }

    let handle = HANDLE(shared_handle as usize as *mut c_void);

    if device == 0 {
        close_owned_handle(handle, owns_handle);
        return Err("device must not be 0".to_string());
    }

    if width == 0 || height == 0 {
        close_owned_handle(handle, owns_handle);
        return Err("width and height must be positive".to_string());
    }
    if plane_count != 1 {
        close_owned_handle(handle, owns_handle);
        return Err("only single-plane DXGI import is supported".to_string());
    }
    if usage == 0 {
        close_owned_handle(handle, owns_handle);
        return Err("usage must not be 0".to_string());
    }
    if keyed_mutex_enabled != 0 {
        close_owned_handle(handle, owns_handle);
        return Err("DXGI keyed mutex import is not implemented yet".to_string());
    }

    let (texture_format, dxgi_format) = match import_format(format) {
        Some(format) => format,
        None => {
            close_owned_handle(handle, owns_handle);
            return Err("format must be bgra8Unorm or bgra8UnormSrgb".to_string());
        }
    };
    let texture_usage = wgpu::TextureUsages::from_bits_truncate(usage);
    if texture_usage.is_empty() {
        close_owned_handle(handle, owns_handle);
        return Err("usage does not contain any supported WGPU texture flags".to_string());
    }

    let entry = unsafe { deref_handle::<DeviceEntry>(device) };
    let hal_device = match unsafe { entry.device.as_hal::<wgpu::hal::api::Dx12>() } {
        Some(hal_device) => hal_device,
        None => {
            close_owned_handle(handle, owns_handle);
            return Err("DXGI shared texture import requires the D3D12 backend".to_string());
        }
    };
    let raw_device = hal_device.raw_device();

    if producer_adapter_luid_low != 0 || producer_adapter_luid_high != 0 {
        let luid = unsafe { raw_device.GetAdapterLuid() };
        if luid.LowPart != producer_adapter_luid_low || luid.HighPart != producer_adapter_luid_high
        {
            close_owned_handle(handle, owns_handle);
            return Err(format!(
                "producer adapter LUID {:08x}:{:08x} does not match renderer adapter LUID {:08x}:{:08x}",
                producer_adapter_luid_high as u32,
                producer_adapter_luid_low,
                luid.HighPart as u32,
                luid.LowPart
            ));
        }
    }

    let mut resource: Option<ID3D12Resource> = None;
    let open_result = unsafe { raw_device.OpenSharedHandle(handle, &mut resource) };
    close_owned_handle(handle, owns_handle);

    open_result.map_err(|error| format!("ID3D12Device::OpenSharedHandle failed: {error}"))?;
    let resource = resource
        .ok_or_else(|| "ID3D12Device::OpenSharedHandle returned no ID3D12Resource".to_string())?;

    let resource_desc = unsafe { resource.GetDesc() };
    if resource_desc.Dimension != D3D12_RESOURCE_DIMENSION_TEXTURE2D {
        return Err("shared resource must be a 2D texture".to_string());
    }
    if resource_desc.Width != width as u64 || resource_desc.Height != height {
        return Err(format!(
            "shared resource dimensions {}x{} do not match descriptor {}x{}",
            resource_desc.Width, resource_desc.Height, width, height
        ));
    }
    if resource_desc.DepthOrArraySize != 1 {
        return Err("shared resource depth/array size must be 1".to_string());
    }
    if resource_desc.MipLevels != 1 {
        return Err("shared resource mip level count must be 1".to_string());
    }
    if resource_desc.SampleDesc.Count != 1 {
        return Err("shared resource sample count must be 1".to_string());
    }
    if resource_desc.Format != dxgi_format {
        return Err(format!(
            "shared resource DXGI format {:?} does not match descriptor {:?}",
            resource_desc.Format, dxgi_format
        ));
    }
    if resource_desc.Layout != D3D12_TEXTURE_LAYOUT_UNKNOWN {
        return Err("shared resource layout must be D3D12_TEXTURE_LAYOUT_UNKNOWN".to_string());
    }
    if !resource_desc
        .Flags
        .contains(D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS)
    {
        return Err(
            "shared resource must set D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS".to_string(),
        );
    }

    let allowed_usage = allowed_usage_for_resource(resource_desc.Flags);
    if !allowed_usage.contains(texture_usage) {
        return Err(format!(
            "requested WGPU usage {:?} exceeds shared resource usage {:?}",
            texture_usage, allowed_usage
        ));
    }

    let size = wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    };
    let texture_desc = wgpu::TextureDescriptor {
        label: Some("Imported DXGI shared texture"),
        size,
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: texture_format,
        usage: texture_usage,
        view_formats: &[],
    };

    let hal_texture = unsafe {
        wgpu::hal::dx12::Device::texture_from_raw(
            resource,
            texture_format,
            wgpu::TextureDimension::D2,
            size,
            1,
            1,
        )
    };
    Ok(unsafe {
        entry
            .device
            .create_texture_from_hal::<wgpu::hal::api::Dx12>(hal_texture, &texture_desc)
    })
}

#[cfg(target_os = "windows")]
#[export_name = "wgpun_DeviceRunD3D12DxgiSharedTextureSyntheticProof"]
pub extern "C" fn wgpuDeviceRunD3D12DxgiSharedTextureSyntheticProof(device: WGPUDevice) -> u8 {
    ffi_catch!(0, {
        match run_d3d12_dxgi_shared_texture_synthetic_proof(device) {
            Ok(()) => 1,
            Err(error) => {
                set_error(error);
                0
            }
        }
    })
}

#[cfg(not(target_os = "windows"))]
#[export_name = "wgpun_DeviceRunD3D12DxgiSharedTextureSyntheticProof"]
pub extern "C" fn wgpuDeviceRunD3D12DxgiSharedTextureSyntheticProof(_device: WGPUDevice) -> u8 {
    set_error("DXGI D3D12 shared texture synthetic proof is only supported on Windows");
    0
}

#[cfg(target_os = "windows")]
#[export_name = "wgpun_DeviceImportDxgiSharedTexture"]
pub extern "C" fn wgpuDeviceImportDxgiSharedTexture(
    device: WGPUDevice,
    shared_handle: u64,
    owns_handle: u8,
    width: u32,
    height: u32,
    format: u32,
    usage: u32,
    plane_count: u32,
    _color_space: u32,
    keyed_mutex_enabled: u8,
    _keyed_mutex_acquire_key: u64,
    _keyed_mutex_release_key: u64,
    _keyed_mutex_timeout_ms: i64,
    producer_adapter_luid_low: u32,
    producer_adapter_luid_high: i32,
) -> WGPUTexture {
    ffi_catch!(0, {
        match import_dxgi_shared_texture(
            device,
            shared_handle,
            owns_handle,
            width,
            height,
            format,
            usage,
            plane_count,
            keyed_mutex_enabled,
            producer_adapter_luid_low,
            producer_adapter_luid_high,
        ) {
            Ok(texture) => into_handle(texture),
            Err(error) => {
                set_error(error);
                0
            }
        }
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
