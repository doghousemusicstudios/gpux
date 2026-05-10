# gpux

Cross-platform GPU packages for Dart and Flutter.

## ShowUp fork note

The `doghousemusicstudios/gpux` fork temporarily carries ShowUp Phase 2.0.5
patches while the upstream PRs are reviewed:

- `naga`: GLSL-to-WGSL translation via `glsl-in` and `wgsl-out`
  (`https://github.com/dartgfx/gpux/pull/3`)
- `wgpu`: external texture import FFI for Metal textures and IOSurface-backed
  textures, plus an Android `AHardwareBuffer` stub that returns an error until
  Vulkan import is device-tested (`https://github.com/dartgfx/gpux/pull/4`)

ShowUp should use the `codex/phase-2-0-5-gpux-ffi` branch in
`dependency_overrides` until equivalent upstream releases are available, then
remove the overrides and return to pub.dev versions.

## Packages

| Package | Description | pub.dev |
|---------|-------------|---------|
| [gpuweb](packages/gpuweb/) | WebGPU-spec interfaces | [![pub](https://img.shields.io/pub/v/gpuweb.svg)](https://pub.dev/packages/gpuweb) |
| [wgpu](packages/wgpu/) | Dart FFI bindings for wgpu | [![pub](https://img.shields.io/pub/v/wgpu.svg)](https://pub.dev/packages/wgpu) |
| [gpuweb_js](packages/gpuweb_js/) | WebGPU bindings for web | [![pub](https://img.shields.io/pub/v/gpuweb_js.svg)](https://pub.dev/packages/gpuweb_js) |
| [gpux](packages/gpux/) | Cross-platform GPU facade | [![pub](https://img.shields.io/pub/v/gpux.svg)](https://pub.dev/packages/gpux) |
| [flutter_wgpu](packages/flutter_wgpu/) | Flutter plugin for wgpu textures | [![pub](https://img.shields.io/pub/v/flutter_wgpu.svg)](https://pub.dev/packages/flutter_wgpu) |
| [flutter_webgpu](packages/flutter_webgpu/) | Flutter widget for WebGPU on web | [![pub](https://img.shields.io/pub/v/flutter_webgpu.svg)](https://pub.dev/packages/flutter_webgpu) |
| [flutter_gpux](packages/flutter_gpux/) | Flutter GPU rendering widget | [![pub](https://img.shields.io/pub/v/flutter_gpux.svg)](https://pub.dev/packages/flutter_gpux) |
| [naga](packages/naga/) | WGSL validation and compilation | [![pub](https://img.shields.io/pub/v/naga.svg)](https://pub.dev/packages/naga) |

## Quick start

```dart
import 'package:gpux/gpux.dart';

final gpu = Gpu();
final adapter = await gpu.requestAdapter();
final device = await adapter.requestDevice();
```

For Flutter, wrap your rendering in a `GpuView`:

```dart
import 'package:flutter_gpux/flutter_gpux.dart';

DefaultGpu(
  child: GpuView(renderer: MyRenderer()),
)
```

See each package's README for details.

## License

MIT
