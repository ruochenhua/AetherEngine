# lighting-pass 归档摘要

## 状态
- 已完成归档
- 提交: a1d8819
- 日期: 2026-05-24

## 已实现内容
1. lighting.wgsl — 全屏 quad Blinn-Phong 光照 shader
2. LightingPass (passes/lighting.rs) — 双 bind group (texture + uniform)
3. Renderer 集成 — GBuffer -> Lighting -> Screen 管线
4. 02_deferred 示例 — 完整窗口渲染，cube + sphere + 方向光

## 验收结果
- cargo check: 0 errors
- cargo clippy: 0 warnings
- cargo test: 2 passed
- cargo run --example 02_deferred: 窗口正常打开，显示 3D 光照效果

## 已知限制
- 相机硬编码 (3,3,3) 看向原点，无交互
- 无阴影、无 IBL、无 SSAO
- 仅支持单方向光
