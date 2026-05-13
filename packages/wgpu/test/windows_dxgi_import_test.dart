import 'dart:io';

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

  group('Windows D3D12 DXGI shared texture synthetic proof', () {
    test('rejects zero device handle before FFI', () {
      final device = WgpuDevice.fromHandle(0);

      expect(
        () => device.windows.runD3D12DxgiSharedTextureSyntheticProof(),
        throwsArgumentError,
      );
    });

    test('reports synthetic proof status', () async {
      if (!Platform.isWindows) {
        final device = WgpuDevice.fromHandle(1);
        final result = device.windows.runD3D12DxgiSharedTextureSyntheticProof();

        expect(
          result.status,
          WgpuWindowsD3D12DxgiSyntheticProofStatus.unsupported,
        );
        expect(result.passed, isFalse);
        expect(result.message, contains('only supported on Windows'));
        return;
      }

      final Wgpu instance;
      try {
        instance = Wgpu.create(
          const WgpuInstanceDescriptor(backends: WgpuBackend.dx12),
        );
      } catch (error) {
        markTestSkipped('D3D12 backend unavailable: $error');
        return;
      }

      WgpuAdapter? adapter;
      try {
        adapter = await instance.requestAdapter();
      } catch (error) {
        instance.dispose();
        markTestSkipped('D3D12 adapter unavailable: $error');
        return;
      }

      try {
        final device = await adapter.requestDevice();
        final result = device.windows.runD3D12DxgiSharedTextureSyntheticProof();

        if (result.status == WgpuWindowsD3D12DxgiSyntheticProofStatus.failed &&
            result.message.contains('requires the D3D12 backend')) {
          markTestSkipped(result.message);
          return;
        }

        expect(
          result.status,
          WgpuWindowsD3D12DxgiSyntheticProofStatus.passed,
          reason: result.message,
        );
        expect(result.passed, isTrue, reason: result.message);
      } finally {
        adapter.dispose();
        instance.dispose();
      }
    });
  });

  group('Windows D3D11 DXGI producer bridge synthetic proof', () {
    test('rejects zero device handle before FFI', () {
      final device = WgpuDevice.fromHandle(0);

      expect(
        () => device.windows.runD3D11DxgiProducerBridgeSyntheticProof(),
        throwsArgumentError,
      );
    });

    test('reports synthetic proof status', () async {
      if (!Platform.isWindows) {
        final device = WgpuDevice.fromHandle(1);
        final result = device.windows
            .runD3D11DxgiProducerBridgeSyntheticProof();

        expect(
          result.status,
          WgpuWindowsD3D11DxgiProducerBridgeProofStatus.unsupported,
        );
        expect(result.passed, isFalse);
        expect(result.message, contains('only supported on Windows'));
        return;
      }

      final Wgpu instance;
      try {
        instance = Wgpu.create(
          const WgpuInstanceDescriptor(backends: WgpuBackend.dx12),
        );
      } catch (error) {
        markTestSkipped('D3D12 backend unavailable: $error');
        return;
      }

      WgpuAdapter? adapter;
      try {
        adapter = await instance.requestAdapter();
      } catch (error) {
        instance.dispose();
        markTestSkipped('D3D12 adapter unavailable: $error');
        return;
      }

      try {
        final device = await adapter.requestDevice();
        final result = device.windows
            .runD3D11DxgiProducerBridgeSyntheticProof();

        if (result.status ==
                WgpuWindowsD3D11DxgiProducerBridgeProofStatus.failed &&
            (result.message.contains('requires the D3D12 backend') ||
                result.message.contains('D3D11CreateDevice failed') ||
                result.message.contains('D3D11On12CreateDevice failed'))) {
          markTestSkipped(result.message);
          return;
        }

        expect(
          result.status,
          WgpuWindowsD3D11DxgiProducerBridgeProofStatus.passed,
          reason: result.message,
        );
        expect(result.passed, isTrue, reason: result.message);
      } finally {
        adapter.dispose();
        instance.dispose();
      }
    });
  });
}
