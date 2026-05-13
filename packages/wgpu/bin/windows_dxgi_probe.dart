import 'dart:convert';
import 'dart:io';

import 'package:wgpu/wgpu.dart';

Future<void> main(List<String> args) async {
  if (!Platform.isWindows) {
    _printJson({
      'platform': Platform.operatingSystem,
      'backend': 'dx12',
      'passed': false,
      'status': 'unsupported',
      'message': 'Windows DXGI probes only run on Windows.',
    });
    return;
  }

  Wgpu? instance;
  WgpuAdapter? adapter;
  try {
    instance = Wgpu.create(
      const WgpuInstanceDescriptor(backends: WgpuBackend.dx12),
    );
    adapter = await instance.requestAdapter();
    final device = await adapter.requestDevice();

    final d3d12 = device.windows.runD3D12DxgiSharedTextureSyntheticProof();
    final d3d11 = device.windows.runD3D11DxgiProducerBridgeSyntheticProof();
    final passed = d3d12.passed && d3d11.passed;

    _printJson({
      'platform': Platform.operatingSystem,
      'backend': 'dx12',
      'passed': passed,
      'proofs': {
        'd3d12DxgiSharedTextureImport': {
          'status': d3d12.status.name,
          'passed': d3d12.passed,
          'message': d3d12.message,
        },
        'd3d11DxgiProducerBridge': {
          'status': d3d11.status.name,
          'passed': d3d11.passed,
          'message': d3d11.message,
        },
      },
    });

    if (!passed) {
      exitCode = 1;
    }
  } catch (error, stackTrace) {
    _printJson({
      'platform': Platform.operatingSystem,
      'backend': 'dx12',
      'passed': false,
      'status': 'failed',
      'message': '$error',
      'stackTrace': '$stackTrace',
    });
    exitCode = 1;
  } finally {
    adapter?.dispose();
    instance?.dispose();
  }
}

void _printJson(Map<String, Object?> payload) {
  stdout.writeln(const JsonEncoder.withIndent('  ').convert(payload));
}
