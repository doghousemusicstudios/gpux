import 'package:test/test.dart';
import 'package:wgpu/wgpu.dart';

void main() {
  group('Windows DXGI shared texture import descriptor validation', () {
    final device = WgpuDevice.fromHandle(1);

    WgpuWindowsDxgiSharedTextureDescriptor descriptor({
      int sharedHandle = 1,
      int width = 64,
      int height = 64,
      GpuTextureFormat format = GpuTextureFormat.bgra8Unorm,
      int planeCount = 1,
      GpuTextureUsageFlags usage = GpuTextureUsage.textureBinding,
      WgpuWindowsKeyedMutexSync? keyedMutex,
    }) {
      return WgpuWindowsDxgiSharedTextureDescriptor(
        sharedHandle: sharedHandle,
        width: width,
        height: height,
        format: format,
        planeCount: planeCount,
        usage: usage,
        keyedMutex: keyedMutex,
      );
    }

    test('rejects zero shared handle before FFI', () {
      expect(
        () =>
            device.windows.importDxgiSharedTexture(descriptor(sharedHandle: 0)),
        throwsArgumentError,
      );
    });

    test('rejects non-positive dimensions before FFI', () {
      expect(
        () => device.windows.importDxgiSharedTexture(descriptor(width: 0)),
        throwsArgumentError,
      );
      expect(
        () => device.windows.importDxgiSharedTexture(descriptor(height: -1)),
        throwsArgumentError,
      );
    });

    test('rejects unsupported format before FFI', () {
      expect(
        () => device.windows.importDxgiSharedTexture(
          descriptor(format: GpuTextureFormat.rgba8Unorm),
        ),
        throwsArgumentError,
      );
    });

    test('rejects zero usage before FFI', () {
      expect(
        () => device.windows.importDxgiSharedTexture(
          descriptor(usage: const GpuTextureUsageFlags(0)),
        ),
        throwsArgumentError,
      );
    });

    test('rejects invalid plane count before FFI', () {
      expect(
        () => device.windows.importDxgiSharedTexture(descriptor(planeCount: 0)),
        throwsArgumentError,
      );
      expect(
        () => device.windows.importDxgiSharedTexture(descriptor(planeCount: 2)),
        throwsArgumentError,
      );
    });

    test('rejects negative keyed mutex timeout before FFI', () {
      expect(
        () => device.windows.importDxgiSharedTexture(
          descriptor(
            keyedMutex: const WgpuWindowsKeyedMutexSync(
              acquireKey: 0,
              releaseKey: 1,
              timeoutMs: -1,
            ),
          ),
        ),
        throwsArgumentError,
      );
    });
  });
}
