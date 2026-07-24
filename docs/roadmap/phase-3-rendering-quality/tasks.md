# Phase 3 执行任务

## Q3.1 法线矩阵

先写非均匀缩放单元测试，确认 normal matrix 与 inverse-transpose 结果一致；再修改 GBuffer、Terrain 和 Water Reflection 的 CPU uniform 构造及 WGSL 输入。奇异矩阵测试必须确认回退行为。

## Q3.2 相机位置

在 `RenderFrame` 的 camera 数据中提供世界空间相机位置，沿 pipeline builder、water reflection pass 和 shader bind group 传递。增加相机平移截图回归，确保反射不是以世界原点为观察点。

## Q3.3 大气 LUT

实现顺序固定为 transmittance → multi-scattering → sky-view → aerial perspective → Sky-to-IBL。每增加一个 LUT，必须注册 Shader manifest、增加尺寸/格式测试，并与旧 raymarch path 做截图比较。

## Q3.4 云透射

在 `crates/aether-engine/src/renderer/passes/volumetric_cloud/shader.rs` 中加入 extinction、step length、累积 transmittance 和 scattering；先保持全分辨率 reference path，再增加半分辨率和时域路径作为可切换 feature。

## Q3.5 性能记录

为大气、云、地形、SSR、合成 Pass 增加 GPU timestamp 或等价计时；将平均值、P95、显存和分辨率写入验证产物。优化不得只以截图主观判断完成。

## Q3.6 验证命令

```text
cargo test -p aether-engine --lib
cargo build -p aether-launcher --release
target/release/aether-launcher.exe --scene scenes/13_clouds.ron --screenshot target/verification/clouds-after.png --exit-after-frames 120 --no-gui-overlay
target/release/aether-launcher.exe --scene scenes/08_terrain.ron --screenshot target/verification/terrain-after.png --exit-after-frames 120 --no-gui-overlay
target/release/aether-launcher.exe --scene scenes/07_ssr_debug.ron --screenshot target/verification/ssr-after.png --exit-after-frames 120 --no-gui-overlay
```
