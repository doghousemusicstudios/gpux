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
use std::mem::ManuallyDrop;
#[cfg(target_os = "windows")]
use std::sync::mpsc;
#[cfg(target_os = "windows")]
use windows::core::{Interface, PCWSTR};
#[cfg(target_os = "windows")]
use windows::Win32::{
    Foundation::{CloseHandle, GENERIC_ALL, HANDLE, HMODULE, LUID, WAIT_OBJECT_0},
    Graphics::{
        Direct3D::D3D_DRIVER_TYPE_UNKNOWN,
        Direct3D11::{
            D3D11CreateDevice, ID3D11Device, ID3D11Device1, ID3D11DeviceContext, ID3D11Resource,
            ID3D11Texture2D, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
            D3D11_CREATE_DEVICE_BGRA_SUPPORT, D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX,
            D3D11_RESOURCE_MISC_SHARED_NTHANDLE, D3D11_SDK_VERSION, D3D11_TEXTURE2D_DESC,
            D3D11_USAGE_DEFAULT,
        },
        Direct3D11on12::{D3D11On12CreateDevice, ID3D11On12Device, D3D11_RESOURCE_FLAGS},
        Direct3D12::{
            ID3D12CommandAllocator, ID3D12CommandList, ID3D12CommandQueue, ID3D12Fence,
            ID3D12GraphicsCommandList, ID3D12Resource, D3D12_COMMAND_LIST_TYPE_DIRECT,
            D3D12_CPU_PAGE_PROPERTY_UNKNOWN, D3D12_FENCE_FLAG_NONE, D3D12_HEAP_FLAG_NONE,
            D3D12_HEAP_FLAG_SHARED, D3D12_HEAP_PROPERTIES, D3D12_HEAP_TYPE_DEFAULT,
            D3D12_HEAP_TYPE_UPLOAD, D3D12_MEMORY_POOL_UNKNOWN, D3D12_PLACED_SUBRESOURCE_FOOTPRINT,
            D3D12_RANGE, D3D12_RESOURCE_DESC, D3D12_RESOURCE_DIMENSION_BUFFER,
            D3D12_RESOURCE_DIMENSION_TEXTURE2D, D3D12_RESOURCE_FLAGS,
            D3D12_RESOURCE_FLAG_ALLOW_RENDER_TARGET, D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS,
            D3D12_RESOURCE_FLAG_ALLOW_UNORDERED_ACCESS, D3D12_RESOURCE_FLAG_NONE,
            D3D12_RESOURCE_STATE_COMMON, D3D12_RESOURCE_STATE_COPY_DEST,
            D3D12_RESOURCE_STATE_GENERIC_READ, D3D12_TEXTURE_COPY_LOCATION,
            D3D12_TEXTURE_COPY_LOCATION_0, D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
            D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX, D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
            D3D12_TEXTURE_LAYOUT_UNKNOWN,
        },
        Dxgi::Common::{
            DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB,
            DXGI_FORMAT_UNKNOWN, DXGI_SAMPLE_DESC,
        },
        Dxgi::{CreateDXGIFactory1, IDXGIAdapter1, IDXGIFactory1, IDXGIKeyedMutex, IDXGIResource1},
    },
    System::Threading::{CreateEventW, WaitForSingleObject, INFINITE},
};

#[cfg(target_os = "windows")]
fn close_owned_handle(handle: HANDLE, owns_handle: u8) {
    if owns_handle != 0 {
        let _ = unsafe { CloseHandle(handle) };
    }
}

#[cfg(target_os = "windows")]
struct OwnedDxgiHandle(HANDLE);

#[cfg(target_os = "windows")]
impl OwnedDxgiHandle {
    fn new(handle: HANDLE) -> Self {
        Self(handle)
    }

    fn value(&self) -> u64 {
        self.0 .0 as usize as u64
    }

    fn handle(&self) -> HANDLE {
        self.0
    }
}

#[cfg(target_os = "windows")]
impl Drop for OwnedDxgiHandle {
    fn drop(&mut self) {
        if !self.0.is_invalid() {
            let _ = unsafe { CloseHandle(self.0) };
        }
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
fn d3d12_synthetic_texture_desc_with_flags(
    width: u32,
    height: u32,
    flags: D3D12_RESOURCE_FLAGS,
) -> D3D12_RESOURCE_DESC {
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
        Flags: flags,
    }
}

#[cfg(target_os = "windows")]
fn synthetic_sentinel_pixels(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = vec![0u8; (width * height * 4) as usize];
    for y in 0..height {
        for x in 0..width {
            let offset = ((y * width + x) * 4) as usize;
            pixels[offset] = (x ^ y) as u8;
            pixels[offset + 1] = y.wrapping_mul(3) as u8;
            pixels[offset + 2] = x.wrapping_mul(5) as u8;
            pixels[offset + 3] = 0xff;
        }
    }
    pixels
}

#[cfg(target_os = "windows")]
fn d3d12_upload_buffer_desc(size: u64) -> D3D12_RESOURCE_DESC {
    D3D12_RESOURCE_DESC {
        Dimension: D3D12_RESOURCE_DIMENSION_BUFFER,
        Alignment: 0,
        Width: size,
        Height: 1,
        DepthOrArraySize: 1,
        MipLevels: 1,
        Format: DXGI_FORMAT_UNKNOWN,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Layout: D3D12_TEXTURE_LAYOUT_ROW_MAJOR,
        Flags: D3D12_RESOURCE_FLAG_NONE,
    }
}

#[cfg(target_os = "windows")]
fn wait_for_d3d12_queue(
    raw_device: &windows::Win32::Graphics::Direct3D12::ID3D12Device,
    raw_queue: &ID3D12CommandQueue,
) -> Result<(), String> {
    let fence: ID3D12Fence = unsafe { raw_device.CreateFence(0, D3D12_FENCE_FLAG_NONE) }
        .map_err(|error| format!("ID3D12Device::CreateFence failed: {error}"))?;
    unsafe { raw_queue.Signal(&fence, 1) }
        .map_err(|error| format!("ID3D12CommandQueue::Signal failed: {error}"))?;

    if unsafe { fence.GetCompletedValue() } >= 1 {
        return Ok(());
    }

    let event = unsafe { CreateEventW(None, false, false, PCWSTR::null()) }
        .map_err(|error| format!("CreateEventW failed: {error}"))?;
    let wait_result = unsafe {
        fence
            .SetEventOnCompletion(1, event)
            .map_err(|error| format!("ID3D12Fence::SetEventOnCompletion failed: {error}"))?;
        WaitForSingleObject(event, INFINITE)
    };
    let _ = unsafe { CloseHandle(event) };

    if wait_result != WAIT_OBJECT_0 {
        return Err(format!(
            "WaitForSingleObject returned unexpected result {:?}",
            wait_result
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn write_d3d12_sentinel_pixels(
    raw_device: &windows::Win32::Graphics::Direct3D12::ID3D12Device,
    raw_queue: &ID3D12CommandQueue,
    resource: &ID3D12Resource,
    texture_desc: &D3D12_RESOURCE_DESC,
    width: u32,
    height: u32,
    sentinel: &[u8],
) -> Result<(), String> {
    let mut layout = D3D12_PLACED_SUBRESOURCE_FOOTPRINT::default();
    let mut row_count = 0u32;
    let mut row_size = 0u64;
    let mut total_bytes = 0u64;
    unsafe {
        raw_device.GetCopyableFootprints(
            texture_desc as *const D3D12_RESOURCE_DESC,
            0,
            1,
            0,
            Some(&mut layout),
            Some(&mut row_count),
            Some(&mut row_size),
            Some(&mut total_bytes),
        );
    }

    let bytes_per_row = (width * 4) as usize;
    if row_count != height {
        return Err(format!(
            "D3D12 copy footprint rows {row_count} do not match texture height {height}"
        ));
    }
    if row_size < bytes_per_row as u64 {
        return Err(format!(
            "D3D12 copy footprint row size {row_size} is smaller than expected {bytes_per_row}"
        ));
    }
    if sentinel.len() != bytes_per_row * height as usize {
        return Err("DXGI synthetic proof sentinel byte count is invalid".to_string());
    }

    let upload_heap = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_UPLOAD,
        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
        CreationNodeMask: 1,
        VisibleNodeMask: 1,
    };
    let upload_desc = d3d12_upload_buffer_desc(total_bytes);
    let mut upload: Option<ID3D12Resource> = None;
    unsafe {
        raw_device.CreateCommittedResource(
            &upload_heap,
            D3D12_HEAP_FLAG_NONE,
            &upload_desc,
            D3D12_RESOURCE_STATE_GENERIC_READ,
            None,
            &mut upload,
        )
    }
    .map_err(|error| format!("ID3D12Device::CreateCommittedResource upload failed: {error}"))?;
    let upload = upload.ok_or_else(|| {
        "ID3D12Device::CreateCommittedResource returned no upload buffer".to_string()
    })?;

    let read_range = D3D12_RANGE { Begin: 0, End: 0 };
    let written_range = D3D12_RANGE {
        Begin: 0,
        End: total_bytes as usize,
    };
    let mut upload_data: *mut c_void = std::ptr::null_mut();
    unsafe {
        upload.Map(
            0,
            Some(&read_range as *const D3D12_RANGE),
            Some(&mut upload_data as *mut *mut c_void),
        )
    }
    .map_err(|error| format!("ID3D12Resource::Map upload failed: {error}"))?;
    if upload_data.is_null() {
        return Err("ID3D12Resource::Map upload returned null".to_string());
    }
    let upload_base = upload_data as *mut u8;
    for y in 0..height as usize {
        let src = sentinel.as_ptr().wrapping_add(y * bytes_per_row);
        let dst = unsafe {
            upload_base.add(layout.Offset as usize + y * layout.Footprint.RowPitch as usize)
        };
        unsafe {
            std::ptr::copy_nonoverlapping(src, dst, bytes_per_row);
        }
    }
    unsafe {
        upload.Unmap(0, Some(&written_range as *const D3D12_RANGE));
    }

    let allocator: ID3D12CommandAllocator =
        unsafe { raw_device.CreateCommandAllocator(D3D12_COMMAND_LIST_TYPE_DIRECT) }
            .map_err(|error| format!("ID3D12Device::CreateCommandAllocator failed: {error}"))?;
    let command_list: ID3D12GraphicsCommandList = unsafe {
        raw_device.CreateCommandList(0, D3D12_COMMAND_LIST_TYPE_DIRECT, &allocator, None)
    }
    .map_err(|error| format!("ID3D12Device::CreateCommandList failed: {error}"))?;

    let mut src_location = D3D12_TEXTURE_COPY_LOCATION {
        pResource: ManuallyDrop::new(Some(upload.clone())),
        Type: D3D12_TEXTURE_COPY_TYPE_PLACED_FOOTPRINT,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
            PlacedFootprint: layout,
        },
    };
    let mut dst_location = D3D12_TEXTURE_COPY_LOCATION {
        pResource: ManuallyDrop::new(Some(resource.clone())),
        Type: D3D12_TEXTURE_COPY_TYPE_SUBRESOURCE_INDEX,
        Anonymous: D3D12_TEXTURE_COPY_LOCATION_0 {
            SubresourceIndex: 0,
        },
    };

    unsafe {
        command_list.CopyTextureRegion(&dst_location, 0, 0, 0, &src_location, None);
        ManuallyDrop::drop(&mut src_location.pResource);
        ManuallyDrop::drop(&mut dst_location.pResource);
    }
    unsafe { command_list.Close() }
        .map_err(|error| format!("ID3D12GraphicsCommandList::Close failed: {error}"))?;
    let command_list_base: ID3D12CommandList = command_list
        .cast()
        .map_err(|error| format!("ID3D12GraphicsCommandList::cast failed: {error}"))?;
    unsafe {
        raw_queue.ExecuteCommandLists(&[Some(command_list_base)]);
    }
    wait_for_d3d12_queue(raw_device, raw_queue)
}

#[cfg(target_os = "windows")]
fn validate_imported_texture_sentinel(
    entry: &DeviceEntry,
    raw_device: &windows::Win32::Graphics::Direct3D12::ID3D12Device,
    raw_queue: &ID3D12CommandQueue,
    resource: &ID3D12Resource,
    texture_desc: &D3D12_RESOURCE_DESC,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) -> Result<(), String> {
    let sentinel = synthetic_sentinel_pixels(width, height);
    initialize_imported_texture_for_readback(entry, texture, width, height);
    write_d3d12_sentinel_pixels(
        raw_device,
        raw_queue,
        resource,
        texture_desc,
        width,
        height,
        &sentinel,
    )?;
    validate_imported_texture_pixels(
        entry,
        texture,
        width,
        height,
        &sentinel,
        "DXGI synthetic proof",
    )
}

#[cfg(target_os = "windows")]
fn initialize_imported_texture_for_readback(
    entry: &DeviceEntry,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
) {
    let bytes_per_row = width * 4;
    let pixels = vec![0u8; (bytes_per_row * height) as usize];
    entry.queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &pixels,
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(bytes_per_row),
            rows_per_image: Some(height),
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    entry.queue.submit(std::iter::empty());
    let _ = entry.device.poll(wgpu::PollType::wait_indefinitely());
}

#[cfg(target_os = "windows")]
fn validate_imported_texture_pixels(
    entry: &DeviceEntry,
    texture: &wgpu::Texture,
    width: u32,
    height: u32,
    expected_pixels: &[u8],
    proof_name: &str,
) -> Result<(), String> {
    let bytes_per_row = width * 4;
    let padded_bytes_per_row = (bytes_per_row + 255) & !255;
    let buffer_size = (padded_bytes_per_row * height) as u64;
    if expected_pixels.len() != (bytes_per_row * height) as usize {
        return Err(format!("{proof_name} expected pixel byte count is invalid"));
    }

    let staging = entry.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("DXGI proof readback"),
        size: buffer_size,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });

    let mut encoder = entry
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("DXGI proof readback encoder"),
        });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &staging,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(padded_bytes_per_row),
                rows_per_image: Some(height),
            },
        },
        wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
    );
    entry.queue.submit(std::iter::once(encoder.finish()));

    let slice = staging.slice(..);
    let (tx, rx) = mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |result| {
        let _ = tx.send(result);
    });
    let _ = entry.device.poll(wgpu::PollType::wait_indefinitely());
    match rx.recv() {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            return Err(format!("{proof_name} readback map_async failed: {error}"));
        }
        Err(_) => return Err(format!("{proof_name} readback channel closed")),
    }

    let mismatch = {
        let mapped = slice.get_mapped_range();
        let mut mismatch = None;
        for y in 0..height {
            let expected_start = (y * bytes_per_row) as usize;
            let actual_start = (y * padded_bytes_per_row) as usize;
            let expected =
                &expected_pixels[expected_start..expected_start + bytes_per_row as usize];
            let actual = &mapped[actual_start..actual_start + bytes_per_row as usize];
            if actual != expected {
                let byte_index = actual
                    .iter()
                    .zip(expected.iter())
                    .position(|(actual, expected)| actual != expected)
                    .unwrap_or(0);
                mismatch = Some((y, byte_index, actual[byte_index], expected[byte_index]));
                break;
            }
        }
        mismatch
    };
    staging.unmap();
    if let Some((row, byte_index, actual, expected)) = mismatch {
        return Err(format!(
            "{proof_name} sentinel mismatch on row {row}, byte {byte_index}: actual {actual:#04x}, expected {expected:#04x}"
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn synthetic_import_usage() -> u32 {
    (wgpu::TextureUsages::TEXTURE_BINDING
        | wgpu::TextureUsages::COPY_SRC
        | wgpu::TextureUsages::COPY_DST)
        .bits()
}

#[cfg(target_os = "windows")]
fn create_d3d12_shared_texture(
    raw_device: &windows::Win32::Graphics::Direct3D12::ID3D12Device,
    width: u32,
    height: u32,
    flags: D3D12_RESOURCE_FLAGS,
) -> Result<(D3D12_RESOURCE_DESC, ID3D12Resource, OwnedDxgiHandle), String> {
    let heap_properties = D3D12_HEAP_PROPERTIES {
        Type: D3D12_HEAP_TYPE_DEFAULT,
        CPUPageProperty: D3D12_CPU_PAGE_PROPERTY_UNKNOWN,
        MemoryPoolPreference: D3D12_MEMORY_POOL_UNKNOWN,
        CreationNodeMask: 1,
        VisibleNodeMask: 1,
    };
    let texture_desc = d3d12_synthetic_texture_desc_with_flags(width, height, flags);
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

    Ok((texture_desc, resource, OwnedDxgiHandle::new(shared_handle)))
}

#[cfg(target_os = "windows")]
fn find_dxgi_adapter_by_luid(
    factory: &IDXGIFactory1,
    target_luid: LUID,
) -> Result<IDXGIAdapter1, String> {
    let mut index = 0;
    loop {
        let adapter = match unsafe { factory.EnumAdapters1(index) } {
            Ok(adapter) => adapter,
            Err(_) => break,
        };
        let desc = unsafe { adapter.GetDesc1() }
            .map_err(|error| format!("IDXGIAdapter1::GetDesc1 failed: {error}"))?;
        if desc.AdapterLuid.LowPart == target_luid.LowPart
            && desc.AdapterLuid.HighPart == target_luid.HighPart
        {
            return Ok(adapter);
        }
        index += 1;
    }
    Err(format!(
        "No DXGI adapter found for renderer LUID {:08x}:{:08x}",
        target_luid.HighPart as u32, target_luid.LowPart
    ))
}

#[cfg(target_os = "windows")]
fn create_d3d11_device_for_luid(
    adapter_luid: LUID,
) -> Result<(ID3D11Device, ID3D11DeviceContext), String> {
    let factory: IDXGIFactory1 =
        unsafe { CreateDXGIFactory1() }.map_err(|error| format!("{error}"))?;
    let adapter = find_dxgi_adapter_by_luid(&factory, adapter_luid)?;

    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    unsafe {
        D3D11CreateDevice(
            &adapter,
            D3D_DRIVER_TYPE_UNKNOWN,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
    }
    .map_err(|error| format!("D3D11CreateDevice failed: {error}"))?;

    let device = device.ok_or_else(|| "D3D11CreateDevice returned no device".to_string())?;
    let context = context.ok_or_else(|| "D3D11CreateDevice returned no context".to_string())?;
    Ok((device, context))
}

#[cfg(target_os = "windows")]
fn create_d3d11_nt_handle_producer(
    adapter_luid: LUID,
    width: u32,
    height: u32,
    sentinel: &[u8],
) -> Result<(ID3D11Texture2D, OwnedDxgiHandle), String> {
    let (device, context) = create_d3d11_device_for_luid(adapter_luid)?;
    let bytes_per_row = width * 4;
    if sentinel.len() != (bytes_per_row * height) as usize {
        return Err("D3D11 producer sentinel byte count is invalid".to_string());
    }

    let desc = D3D11_TEXTURE2D_DESC {
        Width: width,
        Height: height,
        MipLevels: 1,
        ArraySize: 1,
        Format: DXGI_FORMAT_B8G8R8A8_UNORM,
        SampleDesc: DXGI_SAMPLE_DESC {
            Count: 1,
            Quality: 0,
        },
        Usage: D3D11_USAGE_DEFAULT,
        BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
        CPUAccessFlags: 0,
        MiscFlags: (D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0 | D3D11_RESOURCE_MISC_SHARED_KEYEDMUTEX.0)
            as u32,
    };
    let mut texture: Option<ID3D11Texture2D> = None;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut texture)) }
        .map_err(|error| format!("ID3D11Device::CreateTexture2D producer failed: {error}"))?;
    let texture = texture
        .ok_or_else(|| "ID3D11Device::CreateTexture2D returned no producer texture".to_string())?;
    let keyed_mutex: IDXGIKeyedMutex = texture
        .cast()
        .map_err(|error| format!("ID3D11Texture2D::cast<IDXGIKeyedMutex> failed: {error}"))?;
    unsafe {
        keyed_mutex
            .AcquireSync(0, INFINITE)
            .map_err(|error| format!("IDXGIKeyedMutex::AcquireSync producer failed: {error}"))?;
        context.UpdateSubresource(
            &texture,
            0,
            None,
            sentinel.as_ptr() as *const c_void,
            bytes_per_row,
            bytes_per_row * height,
        );
        context.Flush();
        keyed_mutex
            .ReleaseSync(1)
            .map_err(|error| format!("IDXGIKeyedMutex::ReleaseSync producer failed: {error}"))?;
    }

    let dxgi_resource: IDXGIResource1 = texture
        .cast()
        .map_err(|error| format!("ID3D11Texture2D::cast<IDXGIResource1> failed: {error}"))?;
    let handle = unsafe { dxgi_resource.CreateSharedHandle(None, GENERIC_ALL.0, PCWSTR::null()) }
        .map_err(|error| format!("IDXGIResource1::CreateSharedHandle failed: {error}"))?;

    Ok((texture, OwnedDxgiHandle::new(handle)))
}

#[cfg(target_os = "windows")]
fn create_d3d11on12_bridge(
    raw_device: &windows::Win32::Graphics::Direct3D12::ID3D12Device,
    raw_queue: &ID3D12CommandQueue,
) -> Result<(ID3D11Device, ID3D11DeviceContext, ID3D11On12Device), String> {
    let mut device: Option<ID3D11Device> = None;
    let mut context: Option<ID3D11DeviceContext> = None;
    let queues = [Some(raw_queue.cast().map_err(|error| {
        format!("ID3D12CommandQueue::cast<IUnknown> failed: {error}")
    })?)];

    unsafe {
        D3D11On12CreateDevice(
            raw_device,
            D3D11_CREATE_DEVICE_BGRA_SUPPORT.0,
            None,
            Some(&queues),
            0,
            Some(&mut device as *mut _),
            Some(&mut context as *mut _),
            None,
        )
    }
    .map_err(|error| format!("D3D11On12CreateDevice failed: {error}"))?;

    let device = device.ok_or_else(|| "D3D11On12CreateDevice returned no device".to_string())?;
    let context = context.ok_or_else(|| "D3D11On12CreateDevice returned no context".to_string())?;
    let device_11on12: ID3D11On12Device = device
        .cast()
        .map_err(|error| format!("ID3D11Device::cast<ID3D11On12Device> failed: {error}"))?;

    Ok((device, context, device_11on12))
}

#[cfg(target_os = "windows")]
fn copy_d3d11_producer_into_d3d12_resource(
    raw_device: &windows::Win32::Graphics::Direct3D12::ID3D12Device,
    raw_queue: &ID3D12CommandQueue,
    producer_handle: &OwnedDxgiHandle,
    destination: &ID3D12Resource,
) -> Result<(), String> {
    let (bridge_device, bridge_context, bridge_11on12) =
        create_d3d11on12_bridge(raw_device, raw_queue)?;
    let bridge_device1: ID3D11Device1 = bridge_device
        .cast()
        .map_err(|error| format!("ID3D11Device::cast<ID3D11Device1> failed: {error}"))?;
    let producer_texture: ID3D11Texture2D =
        unsafe { bridge_device1.OpenSharedResource1(producer_handle.handle()) }
            .map_err(|error| format!("ID3D11Device1::OpenSharedResource1 failed: {error}"))?;
    let producer_resource: ID3D11Resource = producer_texture
        .cast()
        .map_err(|error| format!("ID3D11Texture2D::cast<ID3D11Resource> failed: {error}"))?;
    let producer_mutex: IDXGIKeyedMutex = producer_texture
        .cast()
        .map_err(|error| format!("ID3D11Texture2D::cast<IDXGIKeyedMutex> failed: {error}"))?;

    let mut wrapped: Option<ID3D11Resource> = None;
    let flags11 = D3D11_RESOURCE_FLAGS::default();
    unsafe {
        bridge_11on12.CreateWrappedResource(
            destination,
            &flags11,
            D3D12_RESOURCE_STATE_COMMON,
            D3D12_RESOURCE_STATE_COMMON,
            &mut wrapped,
        )
    }
    .map_err(|error| format!("ID3D11On12Device::CreateWrappedResource failed: {error}"))?;
    let wrapped = wrapped.ok_or_else(|| {
        "ID3D11On12Device::CreateWrappedResource returned no resource".to_string()
    })?;

    let wrapped_resources = [Some(wrapped.clone())];
    unsafe {
        producer_mutex
            .AcquireSync(1, INFINITE)
            .map_err(|error| format!("IDXGIKeyedMutex::AcquireSync bridge failed: {error}"))?;
        bridge_11on12.AcquireWrappedResources(&wrapped_resources);
        bridge_context.CopyResource(&wrapped, &producer_resource);
        bridge_11on12.ReleaseWrappedResources(&wrapped_resources);
        bridge_context.Flush();
        producer_mutex
            .ReleaseSync(2)
            .map_err(|error| format!("IDXGIKeyedMutex::ReleaseSync bridge failed: {error}"))?;
    }
    wait_for_d3d12_queue(raw_device, raw_queue)
}

#[cfg(target_os = "windows")]
fn import_synthetic_shared_texture(
    device: WGPUDevice,
    shared_handle: &OwnedDxgiHandle,
    width: u32,
    height: u32,
    producer_adapter_luid_low: u32,
    producer_adapter_luid_high: i32,
) -> Result<wgpu::Texture, String> {
    import_dxgi_shared_texture(
        device,
        shared_handle.value(),
        0,
        width,
        height,
        22,
        synthetic_import_usage(),
        1,
        0,
        producer_adapter_luid_low,
        producer_adapter_luid_high,
    )
}

#[cfg(target_os = "windows")]
fn expect_synthetic_import_failure(
    device: WGPUDevice,
    shared_handle: &OwnedDxgiHandle,
    width: u32,
    height: u32,
    format: u32,
    expected_error: &str,
    proof_name: &str,
    producer_adapter_luid_low: u32,
    producer_adapter_luid_high: i32,
) -> Result<(), String> {
    match import_dxgi_shared_texture(
        device,
        shared_handle.value(),
        0,
        width,
        height,
        format,
        synthetic_import_usage(),
        1,
        0,
        producer_adapter_luid_low,
        producer_adapter_luid_high,
    ) {
        Ok(texture) => {
            drop(texture);
            Err(format!(
                "DXGI synthetic proof accepted invalid descriptor for {proof_name}"
            ))
        }
        Err(error) if error.contains(expected_error) => Ok(()),
        Err(error) => Err(format!(
            "DXGI synthetic proof {proof_name} returned unexpected error: {error}"
        )),
    }
}

#[cfg(target_os = "windows")]
fn validate_import_descriptor_rejections(
    device: WGPUDevice,
    raw_device: &windows::Win32::Graphics::Direct3D12::ID3D12Device,
    producer_adapter_luid_low: u32,
    producer_adapter_luid_high: i32,
) -> Result<(), String> {
    let (_, _resource, shared_handle) = create_d3d12_shared_texture(
        raw_device,
        64,
        64,
        D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS,
    )?;
    expect_synthetic_import_failure(
        device,
        &shared_handle,
        63,
        64,
        22,
        "shared resource dimensions",
        "mismatched dimensions",
        producer_adapter_luid_low,
        producer_adapter_luid_high,
    )?;
    expect_synthetic_import_failure(
        device,
        &shared_handle,
        64,
        64,
        23,
        "does not match descriptor",
        "mismatched format",
        producer_adapter_luid_low,
        producer_adapter_luid_high,
    )?;

    let (_, _resource, shared_handle) =
        create_d3d12_shared_texture(raw_device, 64, 64, D3D12_RESOURCE_FLAG_NONE)?;
    expect_synthetic_import_failure(
        device,
        &shared_handle,
        64,
        64,
        22,
        "D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS",
        "missing simultaneous access flag",
        producer_adapter_luid_low,
        producer_adapter_luid_high,
    )
}

#[cfg(target_os = "windows")]
fn validate_import_readback_lifecycle(
    device: WGPUDevice,
    entry: &DeviceEntry,
    raw_device: &windows::Win32::Graphics::Direct3D12::ID3D12Device,
    raw_queue: &ID3D12CommandQueue,
    width: u32,
    height: u32,
    producer_adapter_luid_low: u32,
    producer_adapter_luid_high: i32,
) -> Result<(), String> {
    let (texture_desc, resource, shared_handle) = create_d3d12_shared_texture(
        raw_device,
        width,
        height,
        D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS,
    )?;
    let texture = import_synthetic_shared_texture(
        device,
        &shared_handle,
        width,
        height,
        producer_adapter_luid_low,
        producer_adapter_luid_high,
    )?;
    validate_imported_texture_sentinel(
        entry,
        raw_device,
        raw_queue,
        &resource,
        &texture_desc,
        &texture,
        width,
        height,
    )?;
    drop(texture);
    Ok(())
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
    let raw_queue = hal_device.raw_queue();
    let luid = unsafe { raw_device.GetAdapterLuid() };

    validate_import_descriptor_rejections(device, raw_device, luid.LowPart, luid.HighPart)?;
    validate_import_readback_lifecycle(
        device,
        entry,
        raw_device,
        raw_queue,
        64,
        64,
        luid.LowPart,
        luid.HighPart,
    )?;
    validate_import_readback_lifecycle(
        device,
        entry,
        raw_device,
        raw_queue,
        31,
        17,
        luid.LowPart,
        luid.HighPart,
    )?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn run_d3d11_dxgi_producer_bridge_synthetic_proof(device: WGPUDevice) -> Result<(), String> {
    if device == 0 {
        return Err("device must not be 0".to_string());
    }

    let entry = unsafe { deref_handle::<DeviceEntry>(device) };
    let hal_device = match unsafe { entry.device.as_hal::<wgpu::hal::api::Dx12>() } {
        Some(hal_device) => hal_device,
        None => {
            return Err(
                "DXGI D3D11 producer bridge synthetic proof requires the D3D12 backend".to_string(),
            );
        }
    };
    let raw_device = hal_device.raw_device();
    let raw_queue = hal_device.raw_queue();
    let luid = unsafe { raw_device.GetAdapterLuid() };
    let width = 45;
    let height = 19;
    let sentinel = synthetic_sentinel_pixels(width, height);

    let (_producer_texture, producer_handle) =
        create_d3d11_nt_handle_producer(luid, width, height, &sentinel)?;
    let (_destination_desc, destination, destination_handle) = create_d3d12_shared_texture(
        raw_device,
        width,
        height,
        D3D12_RESOURCE_FLAG_ALLOW_SIMULTANEOUS_ACCESS,
    )?;
    let texture = import_synthetic_shared_texture(
        device,
        &destination_handle,
        width,
        height,
        luid.LowPart,
        luid.HighPart,
    )?;
    initialize_imported_texture_for_readback(entry, &texture, width, height);
    copy_d3d11_producer_into_d3d12_resource(raw_device, raw_queue, &producer_handle, &destination)?;
    validate_imported_texture_pixels(
        entry,
        &texture,
        width,
        height,
        &sentinel,
        "DXGI D3D11 producer bridge proof",
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
#[export_name = "wgpun_DeviceRunD3D11DxgiProducerBridgeSyntheticProof"]
pub extern "C" fn wgpuDeviceRunD3D11DxgiProducerBridgeSyntheticProof(device: WGPUDevice) -> u8 {
    ffi_catch!(0, {
        match run_d3d11_dxgi_producer_bridge_synthetic_proof(device) {
            Ok(()) => 1,
            Err(error) => {
                set_error(error);
                0
            }
        }
    })
}

#[cfg(not(target_os = "windows"))]
#[export_name = "wgpun_DeviceRunD3D11DxgiProducerBridgeSyntheticProof"]
pub extern "C" fn wgpuDeviceRunD3D11DxgiProducerBridgeSyntheticProof(_device: WGPUDevice) -> u8 {
    set_error("DXGI D3D11 producer bridge synthetic proof is only supported on Windows");
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
