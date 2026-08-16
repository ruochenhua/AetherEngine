# Bloom Pass — Task Breakdown

> 状态：✅ 已完成（代码已合入，任务清单归档）

## 任务 1：准备工作 + RED

- [x] 创建 `openspec/specs/bloom-pass/` 及本文档
- [x] 验证 `cargo check` 基线（#65 完成后）
- [x] 编写 bloom 链单元测试骨架：各 Pass signature 验证
- [x] 创建验证场景 `scenes/09_bloom.ron`（强光源 + 高金属度球体 + 暗背景）

**验收标准**：
- `cargo check` 0 errors
- 测试骨架编译通过
- `09_bloom.ron` 可加载

---

## 任务 2：资源标签与 ToneMappingPass 适配

- [x] 在 `resource.rs` 新增：BrightTexture、BloomMip0、BloomMip1、BloomMip2、BloomTexture、BloomResult
- [x] 修改 `tonemapping.rs` signature：read BloomResult 替代 PostProcessInput
- [x] 修改 `tonemapping.rs` resolve：从 "bloom_result" 获取 handle
- [x] 更新 ToneMappingPass 单元测试

**验收标准**：
- `cargo check` 通过
- `cargo test` 中 tonemapping tests 更新并通过

---

## 任务 3：Bloom Extract Pass

- [x] 创建 `bloom_extract.rs`
- [x] 全屏 quad shader：计算 luminance，阈值提取 `(color - threshold) * intensity`
- [x] 实现 Pass trait
- [x] Uniform：threshold (f32), intensity (f32), enabled (u32)
- [x] 添加 signature / init 测试

**验收标准**：
- `cargo check` 通过
- Signature：read PostProcessInput，write BrightTexture

---

## 任务 4：Bloom Downsample Pass（3 级）

- [x] 创建 `bloom_downsample.rs`
- [x] 全屏 quad shader：双线性降采样 + Gaussian blur（5-tap 近似）
- [x] 每个 downsample pass 写不同尺寸的 mip
- [x] 使用 `write_sized` 声明固定尺寸纹理
- [x] 3 个 Pass 实例：输入 BrightTexture→BloomMip0(1/2)→BloomMip1(1/4)→BloomMip2(1/8)

**验收标准**：
- `cargo check` 通过
- PipelineBuilder 能正确分配不同尺寸的 transient texture

---

## 任务 5：Bloom Upsample Pass（3 级）

- [x] 创建 `bloom_upsample.rs`
- [x] 全屏 quad shader：双线性上采样 + 加法 blend
- [x] RenderPass 使用 `blend: Some(Additive)` 或 shader 内手动相加
- [x] 3 个 Pass 实例：BloomMip2→BloomMip1→BloomMip0→BloomTexture

**验收标准**：
- `cargo check` 通过
- 上采样链 signature 正确

---

## 任务 6：Bloom Composite Pass

- [x] 创建 `bloom_composite.rs`
- [x] 全屏 quad shader：读取 PostProcessInput + BloomTexture，输出 `HDR + bloom * intensity`
- [x] Uniform：bloom_intensity (f32), enabled (u32)
- [x] Write BloomResult (Rgba16Float)

**验收标准**：
- `cargo check` 通过
- Signature：reads PostProcessInput + BloomTexture，writes BloomResult

---

## 任务 7：Launcher 集成（管线注册 + UI）

- [x] `passes/mod.rs` 导出新 passes
- [x] `main.rs` PipelineBuilder 注册 bloom 链（6 个 Pass + 修改后的 ToneMappingPass）
- [x] `scheduler.rs` 新增 setters：bloom threshold / intensity / enabled
- [x] Launcher egui 面板添加 Bloom 控件：
  - Checkbox: Enabled
  - Slider: Threshold [0.0, 2.0]
  - Slider: Intensity [0.0, 2.0]

**验收标准**：
- `cargo check` 通过
- Launcher 启动无 panic
- UI 控件可调节参数，画面实时更新

---

## 任务 8：编译验证

- [x] `cargo check --all-targets` 0 errors
- [x] `cargo clippy --all-targets` 无新增 warnings
- [x] `cargo test -p aether-engine` 全部通过

**验收标准**：
- `cargo check` 0 errors, 0 warnings
- 所有测试绿色

---

## 任务 9：运行时验证 + 视觉测试

- [x] 运行 Launcher，加载 `09_bloom.ron`
- [x] 开启 Bloom：确认高金属度球体在光源处有柔和辉光
- [x] 调节 threshold：高 threshold 时辉光减少，低 threshold 时辉光扩散
- [x] 调节 intensity：确认强度线性变化
- [x] 关闭 Bloom：画面与 #65 完全一致（regression 验证）
- [x] Debug grid/gizmo 仍正确显示
- [x] 运行 SMART GATE → MUST_VERIFY
- [x] Agent 读取截图，判定 PASS/FAIL
- [x] 生成视觉测试报告 `tests/reports/YYYY-MM-DD-bloom.md`

**验收标准**：
- Bloom 开启时高亮区域有明显辉光
- Bloom 关闭时与 #65 无差异
- 视觉测试报告已归档
- Issue 可关闭
