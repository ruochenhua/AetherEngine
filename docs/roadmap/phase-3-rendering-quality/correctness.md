# 画质正确性计划

## 法线矩阵

在 `gbuffer.rs`、`water_reflection.rs` 和 `terrain/shaders.rs` 中统一使用模型矩阵上 3×3 的逆转置；奇异矩阵必须回退到单位矩阵并产生诊断。增加非均匀缩放场景，验证法线长度、背面剔除和镜面高光方向。

## 水面反射

新增 camera position uniform，从 Frame 数据传入世界空间相机位置；反射视线使用 `camera_position - world_position`，不能假定相机位于原点。增加相机平移而不旋转的回归场景，确认反射随相机移动而改变。

## 空间约定

在渲染数据文档中固定 world/view/clip/tangent 的方向、矩阵乘法顺序、深度范围和 handedness。所有 Pass 的 uniform 命名必须能表达空间，例如 `world_position`、`view_position`。

## 验证

```text
cargo test -p aether-engine --lib
cargo build -p aether-launcher --release
target/release/aether-launcher.exe --scene scenes/07_ssr_debug.ron --exit-after-frames 120 --screenshot <path> --no-gui-overlay
```
