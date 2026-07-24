# Phase 2：Render Graph 正确性

## 目标

把资源读写从隐式约定升级为可验证的资源版本和生命周期语义。

## 目标模型

每个逻辑资源由 `(ResourceId, Version)` 标识。Pass 声明输入版本、输出新版本及有效策略；Scheduler 只调度满足依赖且输入有效的 Pass。

```text
CloudColor:v3 --read--> Composite
CloudColor:v2 --write--> CloudPass
CloudColor:v3 --clear/fallback--> Composite
```

## 必须定义的策略

- `Clear`：跳过 Pass 时写入确定的中性值。
- `Preserve`：只允许显式声明并保证上一版本仍有效。
- `Fallback`：绑定白色、黑色、天空或默认深度等资源。
- `Invalid`：阻止消费者执行并产生诊断信息。

## 任务

### R2.1 资源版本类型

修改 `crates/aether-engine/src/renderer/pipeline_builder.rs`、资源表和 Pass trait，加入版本生成、读写声明和有效状态。

### R2.2 Scheduler 输出策略

修改 `crates/aether-engine/src/renderer/scheduler.rs`，跳过 Pass 时执行对应 `Clear/Preserve/Fallback`；禁止消费者无条件采样无效输出。

### R2.3 Composite 输入校验

修改 `crates/aether-engine/src/renderer/passes/composite.rs`，对 Cloud、Water、GodRay、Reflection 输出读取有效性；无效时走明确 fallback。

### R2.4 资源兼容性验证

在 CPU 测试中覆盖 format、size、usage、重复写入、未定义读取、场景切换和连续跳帧。

### R2.5 跨场景缓存清理

检查所有 GPU 缓存，在 open/new scene 路径调用 `clear()`；CPU registry 可保留，但 GPU entry 不得跨场景悬挂。

## 退出条件

关闭任一可选效果后连续多帧无上一帧残留；场景切换无旧资源引用；非法图在 CPU 测试中被拒绝。
