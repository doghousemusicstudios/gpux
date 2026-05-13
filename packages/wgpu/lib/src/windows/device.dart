import 'package:gpuweb/gpuweb.dart';

import '../../wgpu_ffi.dart' as wgpu_ffi;
import '../device.dart';
import '../ffi/enum_ffi.dart';
import '../texture.dart';

extension WgpuWindowsDeviceExtension on WgpuDevice {
  WgpuWindowsDevice get windows => WgpuWindowsDevice(this);
}

final class WgpuWindowsDevice {
  const WgpuWindowsDevice(this.device);

  final WgpuDevice device;

  WgpuTexture importDxgiSharedTexture(
    WgpuWindowsDxgiSharedTextureDescriptor descriptor,
  ) {
    descriptor.validate();

    final keyedMutex = descriptor.keyedMutex;
    final handle = wgpu_ffi.wgpun_DeviceImportDxgiSharedTexture(
      device.handle,
      descriptor.sharedHandle,
      descriptor.ownsHandle ? 1 : 0,
      descriptor.width,
      descriptor.height,
      descriptor.format.ffiValue,
      descriptor.usage,
      descriptor.planeCount,
      descriptor.colorSpace.ffiValue,
      keyedMutex == null ? 0 : 1,
      keyedMutex?.acquireKey ?? 0,
      keyedMutex?.releaseKey ?? 0,
      keyedMutex?.timeoutMs ?? 0,
      descriptor.producerAdapterLuidLow ?? 0,
      descriptor.producerAdapterLuidHigh ?? 0,
    );
    if (handle == 0) {
      throw StateError(
        'Failed to import DXGI shared texture: ${wgpu_ffi.wgpuLastError()}',
      );
    }

    return WgpuTexture.internal(
      handle,
      width: descriptor.width,
      height: descriptor.height,
      depthOrArrayLayers: 1,
      dimension: GpuTextureDimension.d2,
      format: descriptor.format,
      usage: descriptor.usage,
      mipLevelCount: 1,
      sampleCount: 1,
      label: descriptor.label,
    );
  }
}

final class WgpuWindowsDxgiSharedTextureDescriptor {
  const WgpuWindowsDxgiSharedTextureDescriptor({
    required this.sharedHandle,
    this.ownsHandle = false,
    required this.width,
    required this.height,
    required this.format,
    this.planeCount = 1,
    this.colorSpace = WgpuWindowsDxgiColorSpace.srgb,
    this.producerAdapterLuidLow,
    this.producerAdapterLuidHigh,
    this.keyedMutex,
    required this.usage,
    this.label = '',
  });

  final int sharedHandle;
  final bool ownsHandle;
  final int width;
  final int height;
  final GpuTextureFormat format;
  final int planeCount;
  final WgpuWindowsDxgiColorSpace colorSpace;
  final int? producerAdapterLuidLow;
  final int? producerAdapterLuidHigh;
  final WgpuWindowsKeyedMutexSync? keyedMutex;
  final GpuTextureUsageFlags usage;
  final String label;

  void validate() {
    if (sharedHandle <= 0) {
      throw ArgumentError('sharedHandle must be positive');
    }
    if (width <= 0) {
      throw ArgumentError('width must be positive');
    }
    if (height <= 0) {
      throw ArgumentError('height must be positive');
    }
    if (!format.isSupportedDxgiImportFormat) {
      throw ArgumentError(
        'format must be bgra8Unorm or bgra8UnormSrgb for DXGI import',
      );
    }
    if (planeCount <= 0) {
      throw ArgumentError('planeCount must be positive');
    }
    if (planeCount != 1) {
      throw ArgumentError('only single-plane DXGI import is supported');
    }
    if (usage == 0) {
      throw ArgumentError('usage must not be 0');
    }
    keyedMutex?.validate();
  }
}

final class WgpuWindowsKeyedMutexSync {
  const WgpuWindowsKeyedMutexSync({
    required this.acquireKey,
    required this.releaseKey,
    required this.timeoutMs,
  });

  final int acquireKey;
  final int releaseKey;
  final int timeoutMs;

  void validate() {
    if (acquireKey < 0) {
      throw ArgumentError('acquireKey must not be negative');
    }
    if (releaseKey < 0) {
      throw ArgumentError('releaseKey must not be negative');
    }
    if (timeoutMs < 0) {
      throw ArgumentError('timeoutMs must not be negative');
    }
  }
}

enum WgpuWindowsDxgiColorSpace { srgb }

extension WgpuWindowsDxgiColorSpaceFfi on WgpuWindowsDxgiColorSpace {
  int get ffiValue => switch (this) {
    WgpuWindowsDxgiColorSpace.srgb => 0,
  };
}

extension WgpuWindowsDxgiImportFormat on GpuTextureFormat {
  bool get isSupportedDxgiImportFormat => switch (this) {
    GpuTextureFormat.bgra8Unorm || GpuTextureFormat.bgra8UnormSrgb => true,
    _ => false,
  };
}
