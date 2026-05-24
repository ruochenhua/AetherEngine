# lighting-pass 归档总结

## 状态
- **变更状态**: 已完成并归档
- **归档日期**: 2026-05-24
- **最终提交**: `4676647`
- **依赖**: gbuffer-pass (已完成)

## 目标
实现 Deferred Shading 的 Lighting Pass：读取 G-Buffer 纹理，计算 Blinn-Phong 光照，输出到屏幕。

## 已完成工作

### T01 - Lighting Shader
- 全屏 quad vertex shader (NDC 空间)
- Fragment shader 采样 4 张 G-Buffer 纹理
- Blinn-Phong 光照模型 (ambient + diffuse + specular)
- 法线解码: `[0,1] -> [-1,1]` via `*2.0-1.0`

### T02 - LightingPass 模块
- `LightingPass` struct 含 pipeline、bind groups、uniform buffer
- 5 个 texture binding (position/normal/albedo/material + sampler)
- 全屏 quad vertex buffer (4 顶点)
- `execute()` 方法接收 `&GBuffer` 和 `&RenderContext`

### T03 - Renderer 集成
- `Renderer` 添加 `lighting_pass: LightingPass` 字段
- `render()` 执行顺序: GBufferPass -> LightingPass -> Screen
- `resize()` 重建 GBuffer 并重新创建 bind group

### T04 - 示例程序
- `02_deferred.rs` 完整窗口渲染示例
- 硬编码相机 `(3,3,3)` 看向原点
- 渲染 cube + sphere
- egui debug overlay (FPS, 帧时间, 分辨率)

### T05 - 编译验证
- `cargo check` 0 errors
- `cargo clippy` 0 warnings
- `cargo test` 2 passed

## 关键修复

| 问题 | 修复提交 |
|------|----------|
| Shader 文件幽灵 (未写入磁盘) | `144377a` - 内联到 Rust 代码 |
| 法线编码/解码缺失 | `144377a` - GBuffer `*0.5+0.5`, Lighting `*2.0-1.0` |
| MRT 多个 fragment entry point | `96cd7ae` - 单 entry 返回 FragmentOutput struct |
| 硬编码 swapchain format | `4676647` - 使用 `config.format` |

## 已知问题
- 光照效果仍需视觉验证（阴阳面是否明显）
- 当前为简化 Blinn-Phong，PBR 后续迭代

## 后续方向
- Scene loader (加载外部模型)
- Camera system (轨道相机/自由相机)
- PBR 光照模型
- Shadow mapping
