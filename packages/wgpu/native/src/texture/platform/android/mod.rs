// AHardwareBuffer -> wgpu::Texture import for Android.
//
// Vulkan-backed only: relies on VK_ANDROID_external_memory_android_hardware_buffer.
// On non-Android targets the FFI symbol still exists but returns 0 + sets the
// last-error string so callers get a deterministic failure instead of a link error.

#[cfg(target_os = "android")]
pub(crate) mod import;

pub(crate) mod exports;
