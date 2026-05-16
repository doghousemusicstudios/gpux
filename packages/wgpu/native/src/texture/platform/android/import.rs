//! Vulkan-backed AHardwareBuffer -> wgpu::Texture import.
//!
//! Relies on `VK_ANDROID_external_memory_android_hardware_buffer`. The device
//! handed in MUST have been created with that extension enabled (the producer
//! side of the FrameSource bridge is responsible for that). If it wasn't, we
//! fail with a precise error string instead of unwinding across FFI.
//!
//! Lifecycle:
//!   1. We `AHardwareBuffer_acquire` the incoming AHB so the caller can drop
//!      its own ref immediately on return.
//!   2. We build a VkImage + dedicated VkDeviceMemory bound to the AHB via
//!      `VkImportAndroidHardwareBufferInfoANDROID`.
//!   3. We hand the VkImage to `wgpu-hal` via `texture_from_raw(..., External)`
//!      with a drop callback that destroys the image, frees the memory, AND
//!      releases the AHB reference. wgpu calls that callback when the public
//!      `wgpu::Texture` is dropped, so reference accounting is automatic.

#![cfg(target_os = "android")]

use ash::android::external_memory_android_hardware_buffer as ahb_ext;
use ash::vk;
use ndk_sys::{
    AHardwareBuffer, AHardwareBuffer_Desc, AHardwareBuffer_Format, AHardwareBuffer_UsageFlags,
    AHardwareBuffer_acquire, AHardwareBuffer_describe, AHardwareBuffer_release,
};
use std::ffi::c_void;

/// RAII wrapper around an `AHardwareBuffer_acquire`'d pointer. Drops via
/// `AHardwareBuffer_release` unless explicitly disarmed with `into_ptr`.
struct AcquiredAhb {
    ptr: *mut AHardwareBuffer,
}

impl AcquiredAhb {
    fn into_ptr(self) -> *mut AHardwareBuffer {
        let ptr = self.ptr;
        std::mem::forget(self);
        ptr
    }
}

impl Drop for AcquiredAhb {
    fn drop(&mut self) {
        unsafe { AHardwareBuffer_release(self.ptr) };
    }
}

fn describe_ahb(ahb: *const AHardwareBuffer) -> AHardwareBuffer_Desc {
    let mut desc = std::mem::MaybeUninit::<AHardwareBuffer_Desc>::zeroed();
    unsafe {
        AHardwareBuffer_describe(ahb, desc.as_mut_ptr());
        desc.assume_init()
    }
}

fn ahb_format_to_wgpu(format: u32) -> Result<wgpu::TextureFormat, String> {
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
        f if f == AHardwareBuffer_Format::AHARDWAREBUFFER_FORMAT_Y8Cb8Cr8_420.0 => Err(
            "YUV AHardwareBuffer formats require VK_KHR_sampler_ycbcr_conversion (not supported yet)"
                .to_string(),
        ),
        other => Err(format!(
            "Unsupported AHardwareBuffer format {other:#x} (supported: R8G8B8A8_UNORM, R8G8B8X8_UNORM, R16G16B16A16_FLOAT)"
        )),
    }
}

fn wgpu_format_to_vk(format: wgpu::TextureFormat) -> Result<vk::Format, String> {
    match format {
        wgpu::TextureFormat::Rgba8Unorm => Ok(vk::Format::R8G8B8A8_UNORM),
        wgpu::TextureFormat::Rgba16Float => Ok(vk::Format::R16G16B16A16_SFLOAT),
        other => Err(format!(
            "AHardwareBuffer import only supports RGBA8/RGBA16F, got {other:?}"
        )),
    }
}

fn find_memory_type(
    instance: &ash::Instance,
    physical_device: vk::PhysicalDevice,
    type_bits: u32,
    flags: vk::MemoryPropertyFlags,
) -> Option<u32> {
    let props = unsafe { instance.get_physical_device_memory_properties(physical_device) };
    props
        .memory_types_as_slice()
        .iter()
        .enumerate()
        .find_map(|(i, mt)| {
            let matches_type = type_bits & (1 << i) != 0;
            let matches_flags = mt.property_flags & flags == flags;
            (matches_type && matches_flags).then_some(i as u32)
        })
}

pub(crate) fn import_ahardware_buffer_to_wgpu(
    device: &wgpu::Device,
    ahb_raw: *mut c_void,
    requested_format: wgpu::TextureFormat,
    width: u32,
    height: u32,
) -> Result<wgpu::Texture, String> {
    if ahb_raw.is_null() {
        return Err("AHardwareBuffer pointer is null".to_string());
    }
    if width == 0 || height == 0 {
        return Err("texture width and height must be positive".to_string());
    }

    // ---- Acquire & validate the AHB ----------------------------------------
    let ahb = ahb_raw.cast::<AHardwareBuffer>();
    unsafe { AHardwareBuffer_acquire(ahb) };
    let acquired = AcquiredAhb { ptr: ahb };

    let desc = describe_ahb(acquired.ptr);
    if desc.width != width || desc.height != height {
        return Err(format!(
            "AHardwareBuffer dims {}x{} do not match requested {}x{}",
            desc.width, desc.height, width, height
        ));
    }
    if desc.layers != 1 {
        return Err(format!(
            "AHardwareBuffer import requires single-layer images, got {} layers",
            desc.layers
        ));
    }
    if desc.usage
        & AHardwareBuffer_UsageFlags::AHARDWAREBUFFER_USAGE_GPU_SAMPLED_IMAGE.0 as u64
        == 0
    {
        return Err(
            "AHardwareBuffer must include AHARDWAREBUFFER_USAGE_GPU_SAMPLED_IMAGE".to_string(),
        );
    }

    let ahb_wgpu_format = ahb_format_to_wgpu(desc.format)?;
    if ahb_wgpu_format != requested_format {
        return Err(format!(
            "AHardwareBuffer format {:#x} maps to {:?}, but caller requested {:?}",
            desc.format, ahb_wgpu_format, requested_format
        ));
    }
    let vk_format = wgpu_format_to_vk(requested_format)?;

    // ---- Reach into wgpu-hal Vulkan ----------------------------------------
    let hal_device = unsafe {
        device
            .as_hal::<wgpu::hal::api::Vulkan>()
            .ok_or_else(|| "wgpu device is not on the Vulkan backend".to_string())?
    };

    if !hal_device
        .enabled_device_extensions()
        .contains(&ahb_ext::NAME)
    {
        return Err(
            "Vulkan device was not created with VK_ANDROID_external_memory_android_hardware_buffer enabled"
                .to_string(),
        );
    }
    let raw_device: ash::Device = hal_device.raw_device().clone();
    let raw_instance: &ash::Instance = hal_device.shared_instance().raw_instance();
    let physical_device: vk::PhysicalDevice = hal_device.raw_physical_device();
    let ahb_loader = ahb_ext::Device::new(raw_instance, &raw_device);

    // ---- Query AHB properties (memory bits, vk format, features) -----------
    //
    // The ash builder pattern returns a struct that mutably borrows
    // `format_properties` (via `pNext`). Capture only the scalar fields we
    // need into `Captured` and drop the chain before re-reading either
    // struct — this sidesteps the lifetime tangle without unsafe.
    struct Captured {
        allocation_size: u64,
        memory_type_bits: u32,
        format: vk::Format,
        format_features: vk::FormatFeatureFlags,
    }
    let captured = {
        let mut format_properties = vk::AndroidHardwareBufferFormatPropertiesANDROID::default();
        let mut properties = vk::AndroidHardwareBufferPropertiesANDROID::default()
            .push_next(&mut format_properties);
        unsafe {
            ahb_loader
                .get_android_hardware_buffer_properties(
                    acquired.ptr.cast::<vk::AHardwareBuffer>(),
                    &mut properties,
                )
                .map_err(|e| {
                    format!("vkGetAndroidHardwareBufferPropertiesANDROID failed: {e:?}")
                })?;
        }
        Captured {
            allocation_size: properties.allocation_size,
            memory_type_bits: properties.memory_type_bits,
            format: format_properties.format,
            format_features: format_properties.format_features,
        }
    };

    if captured.allocation_size == 0 {
        return Err("AHardwareBuffer reports zero allocation size".to_string());
    }
    if captured.format == vk::Format::UNDEFINED {
        return Err(
            "AHardwareBuffer uses an external/YUV-only Vulkan format; YUV import is deferred"
                .to_string(),
        );
    }
    if captured.format != vk_format {
        return Err(format!(
            "AHardwareBuffer Vulkan format {:?} does not match expected {:?}",
            captured.format, vk_format
        ));
    }
    if !captured
        .format_features
        .contains(vk::FormatFeatureFlags::SAMPLED_IMAGE)
    {
        return Err(format!(
            "AHardwareBuffer format {:?} is not sampleable by Vulkan",
            captured.format
        ));
    }

    // ---- Create the VkImage with external-memory chain ----------------------
    let mut external_info = vk::ExternalMemoryImageCreateInfo::default()
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
        .push_next(&mut external_info);

    let image = unsafe { raw_device.create_image(&image_info, None) }
        .map_err(|e| format!("vkCreateImage for AHardwareBuffer failed: {e:?}"))?;

    let mem_reqs = unsafe { raw_device.get_image_memory_requirements(image) };
    let memory_type_bits = mem_reqs.memory_type_bits & captured.memory_type_bits;
    let Some(memory_type_index) = find_memory_type(
        raw_instance,
        physical_device,
        memory_type_bits,
        vk::MemoryPropertyFlags::DEVICE_LOCAL,
    ) else {
        unsafe { raw_device.destroy_image(image, None) };
        return Err(format!(
            "No DEVICE_LOCAL memory type matches AHardwareBuffer bits {memory_type_bits:#x}"
        ));
    };

    // ---- Allocate dedicated memory backed by the AHB -----------------------
    let mut dedicated_info = vk::MemoryDedicatedAllocateInfo::default().image(image);
    let mut import_info =
        vk::ImportAndroidHardwareBufferInfoANDROID::default().buffer(acquired.ptr.cast());
    let allocate_info = vk::MemoryAllocateInfo::default()
        .allocation_size(captured.allocation_size)
        .memory_type_index(memory_type_index)
        .push_next(&mut dedicated_info)
        .push_next(&mut import_info);

    let memory = match unsafe { raw_device.allocate_memory(&allocate_info, None) } {
        Ok(mem) => mem,
        Err(e) => {
            unsafe { raw_device.destroy_image(image, None) };
            return Err(format!(
                "vkAllocateMemory importing AHardwareBuffer failed: {e:?}"
            ));
        }
    };

    if let Err(e) = unsafe { raw_device.bind_image_memory(image, memory, 0) } {
        unsafe {
            raw_device.free_memory(memory, None);
            raw_device.destroy_image(image, None);
        }
        return Err(format!(
            "vkBindImageMemory for imported AHardwareBuffer failed: {e:?}"
        ));
    }

    // ---- Wrap into a wgpu::Texture via wgpu-hal ----------------------------
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
        format: requested_format,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    };
    let hal_desc = wgpu::hal::TextureDescriptor {
        label: Some("External AHardwareBuffer texture"),
        size: public_desc.size,
        mip_level_count: public_desc.mip_level_count,
        sample_count: public_desc.sample_count,
        dimension: public_desc.dimension,
        format: requested_format,
        usage: wgpu::TextureUses::RESOURCE | wgpu::TextureUses::COPY_SRC,
        memory_flags: wgpu::hal::MemoryFlags::empty(),
        view_formats: vec![],
    };

    // Drop callback owns: VkImage, VkDeviceMemory, AHB ref. Runs when wgpu
    // releases the texture handle.
    let drop_device = raw_device.clone();
    let ahb_for_drop = acquired.into_ptr() as usize;
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
