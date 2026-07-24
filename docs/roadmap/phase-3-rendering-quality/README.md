# Phase 3：核心画质与性能

## 目标

先修正明确的空间/光照错误，再将大气和云改造为可控成本的实现。

## 子系统文档

- `correctness.md`：法线、空间和水面反射。
- `atmosphere.md`：LUT 大气与天空驱动 IBL。
- `clouds.md`：物理透射、半分辨率和时域累积。

## 共同要求

- 新 Shader 必须导出 `pub(crate) const` 并加入 `SHADER_MANIFEST`。
- 每次渲染变更运行引擎测试、release build 和相关场景。
- 每个优化前后记录 GPU pass 时间和显存变化。
