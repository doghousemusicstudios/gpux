// Resource creation and release operations.

use super::enums::*;
use super::types::*;
use crate::handle::*;
use std::ffi::c_void;

#[cfg(target_vendor = "apple")]
use objc2::rc::Retained;
#[cfg(target_vendor = "apple")]
use objc2::runtime::{AnyObject, ProtocolObject};
#[cfg(target_vendor = "apple")]
use objc2_metal::{
    MTLDevice, MTLPixelFormat, MTLStorageMode, MTLTexture, MTLTextureDescriptor,
    MTLTextureType, MTLTextureUsage,
};

#[cfg(target_os = "android")]
use ash::{android::external_memory_android_hardware_buffer, vk};
#[cfg(target_os = "android")]
use ndk_sys::{
    AHardwareBuffer, AHardwareBuffer_Desc, AHardwareBuffer_Format, AHardwareBuffer_UsageFlags,
    AHardwareBuffer_acquire, AHardwareBuffer_describe, AHardwareBuffer_release,
};

/// Convert a nullable C string pointer to an Option<&str> for wgpu labels.
pub(crate) unsafe fn label_from_ptr(ptr: *const std::ffi::c_char) -> Option<&'static str> {
    if ptr.is_null() {
        return None;
    }
    std::ffi::CStr::from_ptr(ptr).to_str().ok().filter(|s| !s.is_empty())
}

// =============================================================================
// BUFFER
// =============================================================================

pub fn device_create_buffer(
    device: &wgpu::Device,
    desc: &WGPUBufferDescriptor,
) -> WGPUBuffer {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: unsafe { label_from_ptr(desc.label) },
        size: desc.size,
        usage: wgpu::BufferUsages::from_bits_truncate(desc.usage),
        mapped_at_creation: desc.mapped_at_creation != 0,
    });
    into_handle(buffer)
}

pub fn buffer_release(buffer: WGPUBuffer) {
    if buffer == 0 { return; }
    unsafe { drop_handle::<wgpu::Buffer>(buffer); }
}

// =============================================================================
// TEXTURE
// =============================================================================

pub fn device_create_texture(
    device: &wgpu::Device,
    desc: &WGPUTextureDescriptor,
) -> WGPUTexture {
    let view_formats: Vec<wgpu::TextureFormat> = if desc.view_format_count > 0 && !desc.view_formats.is_null() {
        let slice = unsafe { std::slice::from_raw_parts(desc.view_formats, desc.view_format_count as usize) };
        slice.iter().map(|&f| texture_format_from_u32(f)).collect()
    } else {
        Vec::new()
    };

    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: unsafe { label_from_ptr(desc.label) },
        size: wgpu::Extent3d {
            width: desc.width,
            height: desc.height,
            depth_or_array_layers: desc.depth_or_array_layers,
        },
        mip_level_count: desc.mip_level_count,
        sample_count: desc.sample_count,
        dimension: texture_dimension_from_u32(desc.dimension),
        format: texture_format_from_u32(desc.format),
        usage: wgpu::TextureUsages::from_bits_truncate(desc.usage),
        view_formats: &view_formats,
    });
    into_handle(texture)
}

pub fn texture_create_from_metal_texture(
    device: &wgpu::Device,
    mtl_texture_ptr: *mut c_void,
    format: u32,
    width: u32,
    height: u32,
) -> Result<WGPUTexture, String> {
    #[cfg(target_vendor = "apple")]
    {
        if mtl_texture_ptr.is_null() {
            return Err("Metal texture pointer is null".to_string());
        }
        if width == 0 || height == 0 {
            return Err("texture width and height must be positive".to_string());
        }

        let metal_texture = unsafe {
            Retained::retain(mtl_texture_ptr as *mut ProtocolObject<dyn MTLTexture>)
                .ok_or_else(|| "Failed to retain Metal texture".to_string())?
        };
        import_metal_texture_to_wgpu(
            device,
            metal_texture,
            texture_format_from_u32(format),
            width,
            height,
            Some("External Metal texture"),
        )
        .map(into_handle)
    }

    #[cfg(not(target_vendor = "apple"))]
    {
        let _ = (device, mtl_texture_ptr, format, width, height);
        Err("Metal texture import is only available on Apple platforms".to_string())
    }
}

pub fn texture_create_from_iosurface(
    device: &wgpu::Device,
    io_surface_id: u32,
    format: u32,
    width: u32,
    height: u32,
) -> Result<WGPUTexture, String> {
    #[cfg(target_vendor = "apple")]
    {
        if io_surface_id == 0 {
            return Err("IOSurface id must be non-zero".to_string());
        }
        if width == 0 || height == 0 {
            return Err("texture width and height must be positive".to_string());
        }

        let format = texture_format_from_u32(format);
        let iosurface = unsafe { IOSurfaceLookup(io_surface_id) };
        if iosurface.is_null() {
            return Err(format!("Failed to look up IOSurface id {io_surface_id}"));
        }

        let metal_device = get_metal_device(device)?;
        let metal_texture =
            create_metal_texture_from_iosurface(&metal_device, iosurface, format, width, height);
        unsafe { CFRelease(iosurface as *const c_void); }
        let metal_texture = metal_texture?;
        import_metal_texture_to_wgpu(
            device,
            metal_texture,
            format,
            width,
            height,
            Some("External IOSurface texture"),
        )
        .map(into_handle)
    }

    #[cfg(not(target_vendor = "apple"))]
    {
        let _ = (device, io_surface_id, format, width, height);
        Err("IOSurface texture import is only available on Apple platforms".to_string())
    }
}

pub fn texture_create_from_ahardware_buffer(
    device: &wgpu::Device,
    ahb: *mut c_void,
    format: u32,
    width: u32,
    height: u32,
) -> Result<WGPUTexture, String> {
    #[cfg(target_os = "android")]
    {
        if ahb.is_null() {
            return Err("AHardwareBuffer pointer is null".to_string());
        }
        if width == 0 || height == 0 {
            return Err("texture width and height must be positive".to_string());
        }

        let ahb = ahb.cast::<AHardwareBuffer>();
        unsafe { AHardwareBuffer_acquire(ahb) };
        let acquired = AcquiredAHardwareBuffer { ptr: ahb };

        let ahb_desc = describe_ahardware_buffer(acquired.ptr);
        if ahb_desc.width != width || ahb_desc.height != height {
            return Err(format!(
                "AHardwareBuffer dimensions {}x{} do not match requested texture {}x{}",
                ahb_desc.width, ahb_desc.height, width, height
            ));
        }
        if ahb_desc.layers != 1 {
            return Err(format!(
                "AHardwareBuffer import only supports single-layer images, got {} layers",
                ahb_desc.layers
            ));
        }
        if ahb_desc.usage
            & AHardwareBuffer_UsageFlags::AHARDWAREBUFFER_USAGE_GPU_SAMPLED_IMAGE.0 as u64
            == 0
        {
            return Err(
                "AHardwareBuffer must include AHARDWAREBUFFER_USAGE_GPU_SAMPLED_IMAGE".to_string(),
            );
        }

        let requested_format = texture_format_from_u32(format);
        let ahb_format = ahardware_buffer_format_to_wgpu(ahb_desc.format)?;
        if requested_format != ahb_format {
            return Err(format!(
                "AHardwareBuffer format {:#x} maps to {ahb_format:?}, but caller requested {requested_format:?}",
                ahb_desc.format
            ));
        }

        let texture = import_ahardware_buffer_to_wgpu(device, acquired, ahb_format, width, height)?;
        Ok(into_handle(texture))
    }

    #[cfg(not(target_os = "android"))]
    {
        let _ = (device, ahb, format, width, height);
        Err("AHardwareBuffer texture import is only available on Android".to_string())
    }
}

#[cfg(target_os = "android")]
struct AcquiredAHardwareBuffer {
    ptr: *mut AHardwareBuffer,
}

#[cfg(target_os = "android")]
impl AcquiredAHardwareBuffer {
    fn into_ptr(self) -> *mut AHardwareBuffer {
        let ptr = self.ptr;
        std::mem::forget(self);
        ptr
    }
}

#[cfg(target_os = "android")]
impl Drop for AcquiredAHardwareBuffer {
    fn drop(&mut self) {
        unsafe { AHardwareBuffer_release(self.ptr) };
    }
}

#[cfg(target_os = "android")]
fn describe_ahardware_buffer(ahb: *const AHardwareBuffer) -> AHardwareBuffer_Desc {
    let mut desc = std::mem::MaybeUninit::<AHardwareBuffer_Desc>::zeroed();
    unsafe {
        AHardwareBuffer_describe(ahb, desc.as_mut_ptr());
        desc.assume_init()
    }
}

#[cfg(target_os = "android")]
fn ahardware_buffer_format_to_wgpu(format: u32) -> Result<wgpu::TextureFormat, String> {
    match format {
        f if f == AHardwareBuffer_Format::AHARDWAREBUFFER_FORMAT_R8G8B8A8_UNORM.0 => {
            Ok(wgpu::TextureFormat::Rgba8Unorm)
        }
        f if f == AHardwareBuffer_Format::AHARDWAREBUFFER_FORMAT_R8G8B8X8_UNORM.0 => {
            Ok(wgpu::TextureFormat::Rgba8Unorm)
        }
        f if f == AHardwareBuffer_Format::AHARDWAREBUFFER_FORMAT_R16G16B16A16_FLOAT.0 => {
            Ok(wgpu::TextureFormat::Rgba16Float)
        }
        f if f == AHardwareBuffer_Format::AHARDWAREBUFFER_FORMAT_Y8Cb8Cr8_420.0 => {
            Err("YUV AHardwareBuffer formats need VK_KHR_sampler_ycbcr_conversion and are not supported yet".to_string())
        }
        other => Err(format!(
            "Unsupported AHardwareBuffer format {other:#x}; supported RGBA formats are R8G8B8A8_UNORM, R8G8B8X8_UNORM, and R16G16B16A16_FLOAT"
        )),
    }
}

#[cfg(target_os = "android")]
fn wgpu_format_to_vk_format(format: wgpu::TextureFormat) -> Result<vk::Format, String> {
    match format {
        wgpu::TextureFormat::Rgba8Unorm => Ok(vk::Format::R8G8B8A8_UNORM),
        wgpu::TextureFormat::Rgba16Float => Ok(vk::Format::R16G16B16A16_SFLOAT),
        other => Err(format!(
            "AHardwareBuffer import only supports RGBA8/RGBA16F wgpu formats, got {other:?}"
        )),
    }
}

#[cfg(target_os = "android")]
fn import_ahardware_buffer_to_wgpu(
    device: &wgpu::Device,
    ahb: AcquiredAHardwareBuffer,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> Result<wgpu::Texture, String> {
    let hal_device = unsafe {
        device
            .as_hal::<wgpu::hal::api::Vulkan>()
            .ok_or_else(|| "Failed to get Vulkan device from wgpu".to_string())?
    };

    if !hal_device
        .enabled_device_extensions()
        .contains(&external_memory_android_hardware_buffer::NAME)
    {
        return Err(
            "Vulkan device was not created with VK_ANDROID_external_memory_android_hardware_buffer"
                .to_string(),
        );
    }

    let raw_device = hal_device.raw_device().clone();
    let raw_instance = hal_device.shared_instance().raw_instance();
    let physical_device = hal_device.raw_physical_device();
    let vk_format = wgpu_format_to_vk_format(format)?;
    let ahb_ext =
        external_memory_android_hardware_buffer::Device::new(raw_instance, hal_device.raw_device());

    let mut format_properties = vk::AndroidHardwareBufferFormatPropertiesANDROID::default();
    let mut properties = vk::AndroidHardwareBufferPropertiesANDROID::default();
    properties.p_next = (&mut format_properties as *mut _) as *mut std::ffi::c_void;
    unsafe {
        ahb_ext
            .get_android_hardware_buffer_properties(
                ahb.ptr.cast::<vk::AHardwareBuffer>(),
                &mut properties,
            )
            .map_err(|error| {
                format!("vkGetAndroidHardwareBufferPropertiesANDROID failed: {error:?}")
            })?;
    }

    if properties.allocation_size == 0 {
        return Err("AHardwareBuffer reports zero allocation size".to_string());
    }
    if format_properties.format == vk::Format::UNDEFINED {
        return Err(
            "AHardwareBuffer uses an external/YUV-only Vulkan format; YUV import is deferred"
                .to_string(),
        );
    }
    if format_properties.format != vk_format {
        return Err(format!(
            "AHardwareBuffer Vulkan format {:?} does not match expected {:?}",
            format_properties.format, vk_format
        ));
    }
    if !format_properties
        .format_features
        .contains(vk::FormatFeatureFlags::SAMPLED_IMAGE)
    {
        return Err(format!(
            "AHardwareBuffer format {:?} is not sampleable by Vulkan",
            format_properties.format
        ));
    }
    if !format_properties
        .format_features
        .contains(vk::FormatFeatureFlags::TRANSFER_SRC)
    {
        return Err(format!(
            "AHardwareBuffer format {:?} cannot be exposed as COPY_SRC",
            format_properties.format
        ));
    }

    let mut external_memory_info = vk::ExternalMemoryImageCreateInfo::default()
        .handle_types(vk::ExternalMemoryHandleTypeFlags::ANDROID_HARDWARE_BUFFER_ANDROID);
    let image_info = vk::ImageCreateInfo::default()
        .image_type(vk::ImageType::TYPE_2D)
        .format(vk_format)
        .extent(vk::Extent3D {
            width,
            height,
            depth: 1,
        })
        .mip_levels(1)
        .array_layers(1)
        .samples(vk::SampleCountFlags::TYPE_1)
        .tiling(vk::ImageTiling::OPTIMAL)
        .usage(vk::ImageUsageFlags::SAMPLED | vk::ImageUsageFlags::TRANSFER_SRC)
        .sharing_mode(vk::SharingMode::EXCLUSIVE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .push_next(&mut external_memory_info);

    let image = unsafe { raw_device.create_image(&image_info, None) }
        .map_err(|error| format!("vkCreateImage for AHardwareBuffer failed: {error:?}"))?;
    let memory_requirements = unsafe { raw_device.get_image_memory_requirements(image) };
    let memory_type_bits = memory_requirements.memory_type_bits & properties.memory_type_bits;
    let memory_type_index = find_vulkan_memory_type_index(
        raw_instance,
        physical_device,
        memory_type_bits,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    )
    .ok_or_else(|| {
        unsafe { raw_device.destroy_image(image, None) };
        format!(
            "No DEVICE_LOCAL memory type matches AHardwareBuffer memory bits {memory_type_bits:#x}"
        )
    })?;

    let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
    let mut import_info =
        vk::ImportAndroidHardwareBufferInfoANDROID::default().buffer(ahb.ptr.cast());
    let allocate_info = vk::MemoryAllocateInfo::default()
        .allocation_size(properties.allocation_size)
        .memory_type_index(memory_type_index)
        .push_next(&mut dedicated_info)
        .push_next(&mut import_info);

    let memory = match unsafe { raw_device.allocate_memory(&allocate_info, None) } {
        Ok(memory) => memory,
        Err(error) => {
            unsafe { raw_device.destroy_image(image, None) };
            return Err(format!(
                "vkAllocateMemory importing AHardwareBuffer failed: {error:?}"
            ));
        }
    };

    if let Err(error) = unsafe { raw_device.bind_image_memory(image, memory, 0) } {
        unsafe {
            raw_device.free_memory(memory, None);
            raw_device.destroy_image(image, None);
        }
        return Err(format!(
            "vkBindImageMemory for imported AHardwareBuffer failed: {error:?}"
        ));
    }

    let public_desc = wgpu::TextureDescriptor {
        label: Some("External AHardwareBuffer texture"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    };
    let hal_desc = wgpu::hal::TextureDescriptor {
        label: Some("External AHardwareBuffer texture"),
        size: public_desc.size,
        mip_level_count: public_desc.mip_level_count,
        sample_count: public_desc.sample_count,
        dimension: public_desc.dimension,
        format,
        usage: wgpu::TextureUses::RESOURCE | wgpu::TextureUses::COPY_SRC,
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: vec![],
    };

    let drop_device = raw_device.clone();
    let ahb_for_drop = ahb.into_ptr() as usize;
    let drop_callback: wgpu::hal::DropCallback = Box::new(move || unsafe {
        drop_device.destroy_image(image, None);
        drop_device.free_memory(memory, None);
        AHardwareBuffer_release(ahb_for_drop as *mut AHardwareBuffer);
    });

    let hal_texture = unsafe {
        hal_device.texture_from_raw(
            image,
            &hal_desc,
            Some(drop_callback),
            wgpu::hal::vulkan::TextureMemory::External,
        )
    };

    Ok(unsafe {
        device.create_texture_from_hal::<wgpu::hal::api::Vulkan>(hal_texture, &public_desc)
    })
}

#[cfg(target_os = "android")]
fn find_vulkan_memory_type_index(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    type_bits: u32,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let memory_properties =
        unsafe { instance.get_physical_device_memory_properties(physical_device) };
    memory_properties
        .memory_types_as_slice()
        .iter()
        .enumerate()
        .find_map(|(index, memory_type)| {
            let matches_type = type_bits & (1 << index) != 0;
            let matches_flags = memory_type.property_flags & flags == flags;
            if matches_type && matches_flags {
                Some(index as u32)
            } else {
                None
            }
        })
}

#[cfg(all(test, target_os = "android"))]
mod android_tests {
    use super::*;
    use std::ptr;

    #[test]
    fn imports_manually_allocated_rgba_ahardware_buffer() {
        let width = 64;
        let height = 32;
        let ahb = match allocate_rgba_test_buffer(width, height) {
            Some(ahb) => ahb,
            None => {
                eprintln!("skipping: device cannot allocate RGBA8 GPU-sampled AHardwareBuffer");
                return;
            }
        };

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::VULKAN,
            ..wgpu::InstanceDescriptor::new_without_display_handle()
        });
        let adapter = match pollster::block_on(instance.request_adapter(
            &wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: None,
                force_fallback_adapter: false,
            },
        )) {
            Ok(adapter) => adapter,
            Err(error) => {
                unsafe { ndk_sys::AHardwareBuffer_release(ahb) };
                panic!("failed to request Vulkan adapter: {error}");
            }
        };

        let device_descriptor = wgpu::DeviceDescriptor {
            label: Some("ahb-import-test-device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::default(),
            memory_hints: wgpu::MemoryHints::Performance,
            trace: wgpu::Trace::Off,
            experimental_features: wgpu::ExperimentalFeatures::default(),
        };
        let (device, _queue) = request_vulkan_device_with_ahb_extension(
            &adapter,
            &device_descriptor,
        )
        .unwrap_or_else(|error| {
            unsafe { ndk_sys::AHardwareBuffer_release(ahb) };
            panic!("{error}");
        });

        let handle = texture_create_from_ahardware_buffer(
            &device,
            ahb.cast(),
            17,
            width,
            height,
        )
        .unwrap_or_else(|error| {
            unsafe { ndk_sys::AHardwareBuffer_release(ahb) };
            panic!("AHardwareBuffer import failed: {error}");
        });

        unsafe { ndk_sys::AHardwareBuffer_release(ahb) };

        assert_ne!(handle, 0);
        let texture = unsafe { deref_handle::<wgpu::Texture>(handle) };
        assert_eq!(texture.width(), width);
        assert_eq!(texture.height(), height);
        assert_eq!(texture.format(), wgpu::TextureFormat::Rgba8Unorm);
        assert!(texture.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING));
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        drop(view);
        texture_release(handle);
    }

    fn allocate_rgba_test_buffer(width: u32, height: u32) -> Option<*mut AHardwareBuffer> {
        let desc = AHardwareBuffer_Desc {
            width,
            height,
            layers: 1,
            format: AHardwareBuffer_Format::AHARDWAREBUFFER_FORMAT_R8G8B8A8_UNORM.0,
            usage: AHardwareBuffer_UsageFlags::AHARDWAREBUFFER_USAGE_GPU_SAMPLED_IMAGE.0 as u64,
            stride: 0,
            rfu0: 0,
            rfu1: 0,
        };

        if unsafe { ndk_sys::AHardwareBuffer_isSupported(&desc) } == 0 {
            return None;
        }

        let mut buffer = ptr::null_mut();
        let status = unsafe { ndk_sys::AHardwareBuffer_allocate(&desc, &mut buffer) };
        if status == 0 && !buffer.is_null() {
            Some(buffer)
        } else {
            None
        }
    }

    fn request_vulkan_device_with_ahb_extension(
        adapter: &wgpu::Adapter,
        descriptor: &wgpu::DeviceDescriptor<'_>,
    ) -> Result<(wgpu::Device, wgpu::Queue), String> {
        let Some(vulkan_adapter) = (unsafe { adapter.as_hal::<wgpu::hal::api::Vulkan>() }) else {
            return Err("selected adapter is not Vulkan".to_string());
        };
        if !vulkan_adapter
            .physical_device_capabilities()
            .supports_extension(external_memory_android_hardware_buffer::NAME)
        {
            return Err(
                "Vulkan adapter does not support VK_ANDROID_external_memory_android_hardware_buffer"
                    .to_string(),
            );
        }

        let required_features = descriptor.required_features;
        let required_limits = descriptor.required_limits.clone();
        let memory_hints = descriptor.memory_hints.clone();
        let hal_device = unsafe {
            vulkan_adapter.open_with_callback(
                required_features,
                &required_limits,
                &memory_hints,
                Some(Box::new(|args| {
                    if !args.extensions.contains(&external_memory_android_hardware_buffer::NAME) {
                        args.extensions
                            .push(external_memory_android_hardware_buffer::NAME);
                    }
                })),
            )
        }
        .map_err(|error| format!("{error:?}"))?;

        unsafe { adapter.create_device_from_hal::<wgpu::hal::api::Vulkan>(hal_device, descriptor) }
            .map_err(|error| error.to_string())
    }
}

pub fn texture_create_view(
    texture_handle: WGPUTexture,
    desc: Option<&WGPUTextureViewDescriptor>,
) -> WGPUTextureView {
    if texture_handle == 0 { return 0; }
    let tex = unsafe { deref_handle::<wgpu::Texture>(texture_handle) };

    let view = match desc {
        Some(d) => tex.create_view(&wgpu::TextureViewDescriptor {
            label: unsafe { label_from_ptr(d.label) },
            format: Some(texture_format_from_u32(d.format)),
            dimension: Some(texture_view_dimension_from_u32(d.dimension)),
            usage: if d.usage == 0 {
                None
            } else {
                Some(wgpu::TextureUsages::from_bits_truncate(d.usage))
            },
            aspect: texture_aspect_from_u32(d.aspect),
            base_mip_level: d.base_mip_level,
            mip_level_count: if d.mip_level_count == 0 { None } else { Some(d.mip_level_count) },
            base_array_layer: d.base_array_layer,
            array_layer_count: if d.array_layer_count == 0 { None } else { Some(d.array_layer_count) },
        }),
        None => tex.create_view(&wgpu::TextureViewDescriptor::default()),
    };
    into_handle(view)
}

pub fn texture_release(texture: WGPUTexture) {
    if texture == 0 { return; }
    unsafe { drop_handle::<wgpu::Texture>(texture); }
}

#[cfg(target_vendor = "apple")]
#[repr(C)]
struct __IOSurface(c_void);

#[cfg(target_vendor = "apple")]
type IOSurfaceRef = *mut __IOSurface;

#[cfg(target_vendor = "apple")]
#[link(name = "IOSurface", kind = "framework")]
extern "C" {
    fn IOSurfaceLookup(csid: u32) -> IOSurfaceRef;
}

#[cfg(target_vendor = "apple")]
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: *const c_void);
}

#[cfg(target_vendor = "apple")]
fn get_metal_device(
    device: &wgpu::Device,
) -> Result<Retained<ProtocolObject<dyn MTLDevice>>, String> {
    unsafe {
        let hal_guard = device
            .as_hal::<wgpu::hal::api::Metal>()
            .ok_or_else(|| "Failed to get Metal device from wgpu".to_string())?;

        Ok(hal_guard.raw_device().clone())
    }
}

#[cfg(target_vendor = "apple")]
fn create_metal_texture_from_iosurface(
    device: &ProtocolObject<dyn MTLDevice>,
    iosurface: IOSurfaceRef,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> Result<Retained<ProtocolObject<dyn MTLTexture>>, String> {
    use objc2::msg_send;

    let desc = MTLTextureDescriptor::new();
    unsafe {
        desc.setTextureType(MTLTextureType::Type2D);
        desc.setPixelFormat(metal_pixel_format_from_wgpu_format(format)?);
        desc.setWidth(width as usize);
        desc.setHeight(height as usize);
        desc.setDepth(1);
        desc.setMipmapLevelCount(1);
        desc.setSampleCount(1);
        desc.setArrayLength(1);
        desc.setStorageMode(MTLStorageMode::Shared);
        desc.setUsage(MTLTextureUsage::ShaderRead | MTLTextureUsage::RenderTarget);
    }

    let iosurface_ptr = iosurface as *mut AnyObject;
    let texture_ptr: *mut AnyObject = unsafe {
        let device_ptr = (device as *const _ as *mut AnyObject).as_ref().unwrap();
        let desc_ptr = (&*desc as *const _ as *mut AnyObject).as_ref().unwrap();
        msg_send![device_ptr, newTextureWithDescriptor: desc_ptr, iosurface: iosurface_ptr, plane: 0usize]
    };

    if texture_ptr.is_null() {
        return Err("Failed to create Metal texture from IOSurface".to_string());
    }

    unsafe {
        Retained::from_raw(texture_ptr as *mut ProtocolObject<dyn MTLTexture>)
            .ok_or_else(|| "Failed to retain Metal texture".to_string())
    }
}

#[cfg(target_vendor = "apple")]
fn import_metal_texture_to_wgpu(
    device: &wgpu::Device,
    metal_texture: Retained<ProtocolObject<dyn MTLTexture>>,
    format: wgpu::TextureFormat,
    width: u32,
    height: u32,
    label: Option<&'static str>,
) -> Result<wgpu::Texture, String> {
    let raw_type = metal_texture.textureType();
    let array_layers = (metal_texture.arrayLength() as u32).max(1);
    let mip_levels = (metal_texture.mipmapLevelCount() as u32).max(1);
    let sample_count = (metal_texture.sampleCount() as u32).max(1);
    let usage = metal_usage_to_wgpu_usage(metal_texture.usage());

    let desc = wgpu::TextureDescriptor {
        label,
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: array_layers,
        },
        mip_level_count: mip_levels,
        sample_count,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage,
        view_formats: &[],
    };

    unsafe {
        let hal_texture = wgpu::hal::metal::Device::texture_from_raw(
            metal_texture,
            format,
            raw_type,
            array_layers,
            mip_levels,
            wgpu::hal::CopyExtent {
                width,
                height,
                depth: 1,
            },
        );

        Ok(device.create_texture_from_hal::<wgpu::hal::api::Metal>(hal_texture, &desc))
    }
}

#[cfg(target_vendor = "apple")]
fn metal_usage_to_wgpu_usage(metal_usage: MTLTextureUsage) -> wgpu::TextureUsages {
    let mut usage = wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC;

    if metal_usage.contains(MTLTextureUsage::ShaderWrite) {
        usage |= wgpu::TextureUsages::STORAGE_BINDING;
    }
    if metal_usage.contains(MTLTextureUsage::RenderTarget) {
        usage |= wgpu::TextureUsages::RENDER_ATTACHMENT;
    }
    if metal_usage.contains(MTLTextureUsage::ShaderRead) {
        usage |= wgpu::TextureUsages::TEXTURE_BINDING;
    }

    usage
}

#[cfg(target_vendor = "apple")]
fn metal_pixel_format_from_wgpu_format(
    format: wgpu::TextureFormat,
) -> Result<MTLPixelFormat, String> {
    match format {
        wgpu::TextureFormat::Rgba8Unorm => Ok(MTLPixelFormat::RGBA8Unorm),
        wgpu::TextureFormat::Rgba8UnormSrgb => Ok(MTLPixelFormat::RGBA8Unorm_sRGB),
        wgpu::TextureFormat::Bgra8Unorm => Ok(MTLPixelFormat::BGRA8Unorm),
        wgpu::TextureFormat::Bgra8UnormSrgb => Ok(MTLPixelFormat::BGRA8Unorm_sRGB),
        other => Err(format!(
            "IOSurface import only supports RGBA8/BGRA8 formats, got {other:?}"
        )),
    }
}

pub fn texture_view_release(view: WGPUTextureView) {
    if view == 0 { return; }
    unsafe { drop_handle::<wgpu::TextureView>(view); }
}

// =============================================================================
// SAMPLER
// =============================================================================

pub fn device_create_sampler(
    device: &wgpu::Device,
    desc: &WGPUSamplerDescriptor,
) -> WGPUSampler {
    let compare = if desc.compare == 0 {
        None
    } else {
        Some(compare_function_from_u32(desc.compare - 1))
    };

    let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
        label: unsafe { label_from_ptr(desc.label) },
        address_mode_u: address_mode_from_u32(desc.address_mode_u),
        address_mode_v: address_mode_from_u32(desc.address_mode_v),
        address_mode_w: address_mode_from_u32(desc.address_mode_w),
        mag_filter: filter_mode_from_u32(desc.mag_filter),
        min_filter: filter_mode_from_u32(desc.min_filter),
        mipmap_filter: mipmap_filter_mode_from_u32(desc.mipmap_filter),
        lod_min_clamp: desc.lod_min_clamp,
        lod_max_clamp: desc.lod_max_clamp,
        compare,
        anisotropy_clamp: desc.max_anisotropy,
        border_color: None,
    });
    into_handle(sampler)
}

pub fn sampler_release(sampler: WGPUSampler) {
    if sampler == 0 { return; }
    unsafe { drop_handle::<wgpu::Sampler>(sampler); }
}

// =============================================================================
// SHADER MODULE
// =============================================================================

pub fn device_create_shader_module(
    device: &wgpu::Device,
    source: &str,
    label: Option<&str>,
) -> WGPUShaderModule {
    let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label,
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    into_handle(module)
}

pub fn shader_module_get_compilation_info(
    module_handle: WGPUShaderModule,
) -> Option<String> {
    if module_handle == 0 { return None; }
    let module = unsafe { deref_handle::<wgpu::ShaderModule>(module_handle) };
    let info = pollster::block_on(module.get_compilation_info());

    if info.messages.is_empty() {
        return None;
    }

    let mut result = String::new();
    for msg in &info.messages {
        let type_char = match msg.message_type {
            wgpu::CompilationMessageType::Error => 'E',
            wgpu::CompilationMessageType::Warning => 'W',
            wgpu::CompilationMessageType::Info => 'I',
        };
        let line_num = msg.location.as_ref().map_or(0, |l| l.line_number);
        let line_pos = msg.location.as_ref().map_or(0, |l| l.line_position);
        if !result.is_empty() {
            result.push('\n');
        }
        result.push(type_char);
        result.push('\t');
        result.push_str(&line_num.to_string());
        result.push('\t');
        result.push_str(&line_pos.to_string());
        result.push('\t');
        result.push_str(&msg.message);
    }

    Some(result)
}

pub fn shader_module_release(module: WGPUShaderModule) {
    if module == 0 { return; }
    unsafe { drop_handle::<wgpu::ShaderModule>(module); }
}

// =============================================================================
// BIND GROUP LAYOUT
// =============================================================================

pub fn device_create_bind_group_layout(
    device: &wgpu::Device,
    desc: &WGPUBindGroupLayoutDescriptor,
) -> WGPUBindGroupLayout {
    let entries: Vec<wgpu::BindGroupLayoutEntry> = if desc.entry_count > 0 && !desc.entries.is_null() {
        let raw_entries = unsafe {
            std::slice::from_raw_parts(desc.entries, desc.entry_count as usize)
        };
        raw_entries.iter().map(|e| {
            let binding_type = match e.binding_type {
                BINDING_TYPE_BUFFER => wgpu::BindingType::Buffer {
                    ty: buffer_binding_type_from_u32(e.buffer_type),
                    has_dynamic_offset: e.has_dynamic_offset != 0,
                    min_binding_size: if e.min_binding_size > 0 {
                        std::num::NonZeroU64::new(e.min_binding_size)
                    } else {
                        None
                    },
                },
                BINDING_TYPE_SAMPLER => wgpu::BindingType::Sampler(
                    sampler_binding_type_from_u32(e.sampler_type)
                ),
                BINDING_TYPE_TEXTURE => wgpu::BindingType::Texture {
                    sample_type: texture_sample_type_from_u32(e.texture_sample_type),
                    view_dimension: texture_view_dimension_from_u32(e.texture_view_dimension),
                    multisampled: e.texture_multisampled != 0,
                },
                BINDING_TYPE_STORAGE_TEXTURE => wgpu::BindingType::StorageTexture {
                    access: storage_texture_access_from_u32(e.buffer_type),
                    format: texture_format_from_u32(e.texture_sample_type),
                    view_dimension: texture_view_dimension_from_u32(e.texture_view_dimension),
                },
                _ => wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
            };
            wgpu::BindGroupLayoutEntry {
                binding: e.binding,
                visibility: shader_stages_from_u32(e.visibility),
                ty: binding_type,
                count: if e.count > 0 { std::num::NonZeroU32::new(e.count) } else { None },
            }
        }).collect()
    } else {
        vec![]
    };

    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: unsafe { label_from_ptr(desc.label) },
        entries: &entries,
    });
    into_handle(layout)
}

pub fn bind_group_layout_release(layout: WGPUBindGroupLayout) {
    if layout == 0 { return; }
    unsafe { drop_handle::<wgpu::BindGroupLayout>(layout); }
}

// =============================================================================
// BIND GROUP
// =============================================================================

pub fn device_create_bind_group(
    device: &wgpu::Device,
    desc: &WGPUBindGroupDescriptor,
) -> WGPUBindGroup {
    if desc.layout == 0 { return 0; }
    let layout = unsafe { deref_handle::<wgpu::BindGroupLayout>(desc.layout) };

    if desc.entry_count == 0 || desc.entries.is_null() {
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: unsafe { label_from_ptr(desc.label) },
            layout,
            entries: &[],
        });
        return into_handle(bind_group);
    }

    let raw_entries = unsafe {
        std::slice::from_raw_parts(desc.entries, desc.entry_count as usize)
    };

    // Pre-collect texture view arrays so they outlive the BindGroupEntry references
    let view_arrays: Vec<Option<Vec<&wgpu::TextureView>>> = raw_entries.iter().map(|e| {
        if e.resource_type == 4 {
            let count = e.size as usize;
            let handles_ptr = e.resource as *const u64;
            let handle_slice = unsafe { std::slice::from_raw_parts(handles_ptr, count) };
            Some(handle_slice
                .iter()
                .filter(|&&h| h != 0)
                .map(|&h| unsafe { deref_handle::<wgpu::TextureView>(h) })
                .collect())
        } else {
            None
        }
    }).collect();

    let entries: Vec<wgpu::BindGroupEntry> = raw_entries.iter().enumerate().filter_map(|(i, e)| {
        let resource = match e.resource_type {
            0 => {
                if e.resource == 0 { return None; }
                let buffer = unsafe { deref_handle::<wgpu::Buffer>(e.resource) };
                let size = if e.size > 0 {
                    std::num::NonZeroU64::new(e.size)
                } else {
                    None
                };
                wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer,
                    offset: e.offset,
                    size,
                })
            }
            1 => {
                if e.resource == 0 { return None; }
                let sampler = unsafe { deref_handle::<wgpu::Sampler>(e.resource) };
                wgpu::BindingResource::Sampler(sampler)
            }
            2 => {
                if e.resource == 0 { return None; }
                let view = unsafe { deref_handle::<wgpu::TextureView>(e.resource) };
                wgpu::BindingResource::TextureView(view)
            }
            4 => {
                let views = view_arrays[i].as_ref()?;
                wgpu::BindingResource::TextureViewArray(views)
            }
            _ => return None,
        };
        Some(wgpu::BindGroupEntry {
            binding: e.binding,
            resource,
        })
    }).collect();

    let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: unsafe { label_from_ptr(desc.label) },
        layout,
        entries: &entries,
    });
    into_handle(bind_group)
}

pub fn bind_group_release(group: WGPUBindGroup) {
    if group == 0 { return; }
    unsafe { drop_handle::<wgpu::BindGroup>(group); }
}

// =============================================================================
// PIPELINE LAYOUT
// =============================================================================

pub fn device_create_pipeline_layout(
    device: &wgpu::Device,
    desc: &WGPUPipelineLayoutDescriptor,
) -> WGPUPipelineLayout {
    let layouts: Vec<Option<&wgpu::BindGroupLayout>> = if desc.bind_group_layout_count > 0 && !desc.bind_group_layouts.is_null() {
        let raw_layouts = unsafe {
            std::slice::from_raw_parts(desc.bind_group_layouts, desc.bind_group_layout_count as usize)
        };
        raw_layouts.iter()
            .map(|&id| if id == 0 { None } else { Some(unsafe { deref_handle::<wgpu::BindGroupLayout>(id) }) })
            .collect()
    } else {
        vec![]
    };

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: unsafe { label_from_ptr(desc.label) },
        bind_group_layouts: &layouts,
        immediate_size: desc.immediate_size,
    });
    into_handle(pipeline_layout)
}

pub fn pipeline_layout_release(layout: WGPUPipelineLayout) {
    if layout == 0 { return; }
    unsafe { drop_handle::<wgpu::PipelineLayout>(layout); }
}

// =============================================================================
// RENDER PIPELINE
// =============================================================================

pub fn device_create_render_pipeline(
    device: &wgpu::Device,
    desc: &WGPURenderPipelineDescriptor,
) -> WGPURenderPipeline {
    if desc.vertex.module == 0 { return 0; }
    let vertex_module = unsafe { deref_handle::<wgpu::ShaderModule>(desc.vertex.module) };

    let mut all_attributes: Vec<Vec<wgpu::VertexAttribute>> = vec![];
    if desc.vertex.buffer_count > 0 && !desc.vertex.buffers.is_null() {
        let raw_buffers = unsafe {
            std::slice::from_raw_parts(desc.vertex.buffers, desc.vertex.buffer_count as usize)
        };
        for b in raw_buffers {
            let attributes: Vec<wgpu::VertexAttribute> = if b.attribute_count > 0 && !b.attributes.is_null() {
                let raw_attrs = unsafe {
                    std::slice::from_raw_parts(b.attributes, b.attribute_count as usize)
                };
                raw_attrs.iter().map(|a| wgpu::VertexAttribute {
                    format: vertex_format_from_u32(a.format),
                    offset: a.offset,
                    shader_location: a.shader_location,
                }).collect()
            } else {
                vec![]
            };
            all_attributes.push(attributes);
        }
    }

    let vertex_buffer_layouts: Vec<wgpu::VertexBufferLayout> = if desc.vertex.buffer_count > 0 && !desc.vertex.buffers.is_null() {
        let raw_buffers = unsafe {
            std::slice::from_raw_parts(desc.vertex.buffers, desc.vertex.buffer_count as usize)
        };
        raw_buffers.iter().enumerate().map(|(i, b)| {
            wgpu::VertexBufferLayout {
                array_stride: b.array_stride,
                step_mode: vertex_step_mode_from_u32(b.step_mode),
                attributes: &all_attributes[i],
            }
        }).collect()
    } else {
        vec![]
    };

    let vertex_entry_point = if !desc.vertex.entry_point.is_null() {
        unsafe { std::ffi::CStr::from_ptr(desc.vertex.entry_point).to_str().unwrap_or("main") }
    } else {
        "main"
    };

    let fragment_module = if desc.fragment.module != 0 {
        Some(unsafe { deref_handle::<wgpu::ShaderModule>(desc.fragment.module) })
    } else {
        None
    };

    let mut color_targets: Vec<Option<wgpu::ColorTargetState>> = vec![];
    if desc.fragment.target_count > 0 && !desc.fragment.targets.is_null() {
        let raw_targets = unsafe {
            std::slice::from_raw_parts(desc.fragment.targets, desc.fragment.target_count as usize)
        };
        for t in raw_targets {
            let blend = if t.blend_enabled != 0 {
                Some(wgpu::BlendState {
                    color: wgpu::BlendComponent {
                        src_factor: blend_factor_from_u32(t.blend_color.src_factor),
                        dst_factor: blend_factor_from_u32(t.blend_color.dst_factor),
                        operation: blend_operation_from_u32(t.blend_color.operation),
                    },
                    alpha: wgpu::BlendComponent {
                        src_factor: blend_factor_from_u32(t.blend_alpha.src_factor),
                        dst_factor: blend_factor_from_u32(t.blend_alpha.dst_factor),
                        operation: blend_operation_from_u32(t.blend_alpha.operation),
                    },
                })
            } else {
                None
            };
            color_targets.push(Some(wgpu::ColorTargetState {
                format: texture_format_from_u32(t.format),
                blend,
                write_mask: color_writes_from_u32(t.write_mask),
            }));
        }
    }

    let fragment_entry_point = if !desc.fragment.entry_point.is_null() {
        unsafe { std::ffi::CStr::from_ptr(desc.fragment.entry_point).to_str().unwrap_or("main") }
    } else {
        "main"
    };

    let depth_stencil = if desc.depth_stencil_enabled != 0 {
        Some(wgpu::DepthStencilState {
            format: texture_format_from_u32(desc.depth_stencil.format),
            depth_write_enabled: Some(desc.depth_stencil.depth_write_enabled != 0),
            depth_compare: Some(compare_function_from_u32(desc.depth_stencil.depth_compare - 1)),
            stencil: wgpu::StencilState {
                front: wgpu::StencilFaceState {
                    compare: if desc.depth_stencil.stencil_front_compare == 0 {
                        wgpu::CompareFunction::Always
                    } else {
                        compare_function_from_u32(desc.depth_stencil.stencil_front_compare - 1)
                    },
                    fail_op: stencil_operation_from_u32(desc.depth_stencil.stencil_front_fail_op),
                    depth_fail_op: stencil_operation_from_u32(desc.depth_stencil.stencil_front_depth_fail_op),
                    pass_op: stencil_operation_from_u32(desc.depth_stencil.stencil_front_pass_op),
                },
                back: wgpu::StencilFaceState {
                    compare: if desc.depth_stencil.stencil_back_compare == 0 {
                        wgpu::CompareFunction::Always
                    } else {
                        compare_function_from_u32(desc.depth_stencil.stencil_back_compare - 1)
                    },
                    fail_op: stencil_operation_from_u32(desc.depth_stencil.stencil_back_fail_op),
                    depth_fail_op: stencil_operation_from_u32(desc.depth_stencil.stencil_back_depth_fail_op),
                    pass_op: stencil_operation_from_u32(desc.depth_stencil.stencil_back_pass_op),
                },
                read_mask: if desc.depth_stencil.stencil_read_mask == 0 { 0xFFFFFFFF } else { desc.depth_stencil.stencil_read_mask },
                write_mask: if desc.depth_stencil.stencil_write_mask == 0 { 0xFFFFFFFF } else { desc.depth_stencil.stencil_write_mask },
            },
            bias: wgpu::DepthBiasState {
                constant: desc.depth_stencil.depth_bias,
                slope_scale: desc.depth_stencil.depth_bias_slope_scale,
                clamp: desc.depth_stencil.depth_bias_clamp,
            },
        })
    } else {
        None
    };

    let layout = if desc.layout != 0 {
        Some(unsafe { deref_handle::<wgpu::PipelineLayout>(desc.layout) })
    } else {
        None
    };

    let owned_vertex_constants = parse_constants(
        desc.vertex.constant_count, desc.vertex.constant_keys, desc.vertex.constant_values,
    );
    let vertex_constants_refs: Vec<(&str, f64)> = owned_vertex_constants.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let vertex_compilation = wgpu::PipelineCompilationOptions {
        constants: &vertex_constants_refs,
        zero_initialize_workgroup_memory: true,
    };

    let owned_fragment_constants = parse_constants(
        desc.fragment.constant_count, desc.fragment.constant_keys, desc.fragment.constant_values,
    );
    let fragment_constants_refs: Vec<(&str, f64)> = owned_fragment_constants.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let fragment_compilation = wgpu::PipelineCompilationOptions {
        constants: &fragment_constants_refs,
        zero_initialize_workgroup_memory: true,
    };

    let fragment_state = fragment_module.map(|module| {
        wgpu::FragmentState {
            module,
            entry_point: Some(fragment_entry_point),
            targets: &color_targets,
            compilation_options: fragment_compilation,
        }
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: unsafe { label_from_ptr(desc.label) },
        layout,
        vertex: wgpu::VertexState {
            module: vertex_module,
            entry_point: Some(vertex_entry_point),
            buffers: &vertex_buffer_layouts,
            compilation_options: vertex_compilation,
        },
        primitive: wgpu::PrimitiveState {
            topology: primitive_topology_from_u32(desc.primitive_topology),
            strip_index_format: index_format_from_u32(desc.strip_index_format),
            front_face: front_face_from_u32(desc.front_face),
            cull_mode: cull_mode_from_u32(desc.cull_mode),
            unclipped_depth: desc.unclipped_depth != 0,
            polygon_mode: wgpu::PolygonMode::Fill,
            conservative: false,
        },
        depth_stencil,
        multisample: wgpu::MultisampleState {
            count: desc.multisample_count.max(1),
            mask: if desc.multisample_mask == 0 { !0 } else { desc.multisample_mask as u64 },
            alpha_to_coverage_enabled: desc.alpha_to_coverage_enabled != 0,
        },
        fragment: fragment_state,
        multiview_mask: None,
        cache: None,
    });
    into_handle(pipeline)
}

pub fn render_pipeline_get_bind_group_layout(
    pipeline_handle: WGPURenderPipeline,
    index: u32,
) -> WGPUBindGroupLayout {
    if pipeline_handle == 0 { return 0; }
    let pipeline = unsafe { deref_handle::<wgpu::RenderPipeline>(pipeline_handle) };
    let layout = pipeline.get_bind_group_layout(index);
    into_handle(layout)
}

pub fn render_pipeline_release(pipeline: WGPURenderPipeline) {
    if pipeline == 0 { return; }
    unsafe { drop_handle::<wgpu::RenderPipeline>(pipeline); }
}

// =============================================================================
// COMPUTE PIPELINE
// =============================================================================

fn parse_constants(count: u32, keys: *const *const std::ffi::c_char, values: *const f64) -> Vec<(String, f64)> {
    let mut constants = Vec::new();
    if count > 0 && !keys.is_null() && !values.is_null() {
        let key_ptrs = unsafe { std::slice::from_raw_parts(keys, count as usize) };
        let vals = unsafe { std::slice::from_raw_parts(values, count as usize) };
        for i in 0..count as usize {
            if !key_ptrs[i].is_null() {
                if let Ok(key) = unsafe { std::ffi::CStr::from_ptr(key_ptrs[i]).to_str() } {
                    constants.push((key.to_string(), vals[i]));
                }
            }
        }
    }
    constants
}

pub fn device_create_compute_pipeline(
    device: &wgpu::Device,
    desc: &WGPUComputePipelineDescriptor,
) -> WGPUComputePipeline {
    if desc.module == 0 { return 0; }
    let module = unsafe { deref_handle::<wgpu::ShaderModule>(desc.module) };

    let entry_point = if !desc.entry_point.is_null() {
        unsafe { std::ffi::CStr::from_ptr(desc.entry_point).to_str().unwrap_or("main") }
    } else {
        "main"
    };

    let layout = if desc.layout != 0 {
        Some(unsafe { deref_handle::<wgpu::PipelineLayout>(desc.layout) })
    } else {
        None
    };

    let owned_constants = parse_constants(desc.constant_count, desc.constant_keys, desc.constant_values);
    let constants_refs: Vec<(&str, f64)> = owned_constants.iter().map(|(k, v)| (k.as_str(), *v)).collect();
    let compilation_options = wgpu::PipelineCompilationOptions {
        constants: &constants_refs,
        zero_initialize_workgroup_memory: true,
    };

    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: unsafe { label_from_ptr(desc.label) },
        layout,
        module,
        entry_point: Some(entry_point),
        compilation_options,
        cache: None,
    });
    into_handle(pipeline)
}

pub fn compute_pipeline_get_bind_group_layout(
    pipeline_handle: WGPUComputePipeline,
    index: u32,
) -> WGPUBindGroupLayout {
    if pipeline_handle == 0 { return 0; }
    let pipeline = unsafe { deref_handle::<wgpu::ComputePipeline>(pipeline_handle) };
    let layout = pipeline.get_bind_group_layout(index);
    into_handle(layout)
}

pub fn compute_pipeline_release(pipeline: WGPUComputePipeline) {
    if pipeline == 0 { return; }
    unsafe { drop_handle::<wgpu::ComputePipeline>(pipeline); }
}
