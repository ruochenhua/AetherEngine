# Phase 2 执行任务

## R2.1 资源版本 API

**修改范围：**

- `crates/aether-engine/src/renderer/pipeline_builder.rs`
- `crates/aether-engine/src/renderer/scheduler.rs`
- `crates/aether-engine/src/renderer/resource_table.rs`（若实际资源表名称不同，以现有模块为准）

建议接口：

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub struct ResourceVersion {
    pub id: ResourceId,
    pub generation: u32,
}

pub enum OutputPolicy {
    Clear,
    Preserve(ResourceVersion),
    Fallback(ResourceFallback),
    Invalid,
}
```

`PassNode` 声明输入 `ResourceVersion` 和输出策略；Builder 在图构建阶段拒绝读取未定义版本。

## R2.2 Scheduler 跳过策略

新增测试覆盖：

1. Pass 被禁用后消费者收到 clear 输出。
2. 明确声明 preserve 时只读取上一版本。
3. fallback 输出不触发 GPU 无效 bind group。
4. invalid 输出使消费者跳过并产生诊断。

必须使用 wholesale replacement 替换 GPU 纹理和 bind group。

## R2.3 Composite 安全读取

**修改文件：** `crates/aether-engine/src/renderer/passes/composite.rs`

将 Cloud、Water、GodRay、Reflection 的读取改为通过有效性查询取得；禁止直接假定资源总是存在。每个 fallback 必须有固定颜色/纹理和单元测试。

## R2.4 场景切换

在 open/new scene 路径加入 GPU cache `clear()`，并增加两次场景切换测试：`clouds → terrain → clouds`。验证旧场景纹理不会被新场景 id 复用。

## R2.5 验证命令

```text
cargo test -p aether-engine --lib
cargo build -p aether-launcher --release
target/release/aether-launcher.exe --scene scenes/13_clouds.ron --exit-after-frames 120 --no-gui-overlay
```
