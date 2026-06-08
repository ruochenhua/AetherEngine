# Tone Mapping Pass — Task Breakdown

## 任务 1：准备工作 + RED（测试与验证场景）

- [ ] 创建 `openspec/specs/tone-mapping-pass/` 及本文档
- [ ] 验证 `cargo check` 基线通过
- [ ] 编写 ToneMappingPass 单元测试骨架：`signature_declares_correct_resources`、`init_does_not_panic`
- [ ] 创建验证场景 `scenes/08_tonemapping.ron`（高曝光 HDR 场景：强光源 + 高金属度球体）

**验收标准**：
- `cargo check` 0 errors
- 测试骨架编译通过（此时应失败/空实现）
- `08_tonemapping.ron` 可被 Launcher 加载

---

## 任务 2：LightingPass 移除 tone mapping

- [ ] 从 `lighting.rs` shader 中移除 tone mapping 段落（`mapped = select(...)` 块）
- [ ] 改为直接 `return vec4<f32>(output_color, 1.0)`
- [ ] 验证 LightingPass 输出仍为 `Rgba16Float`

**验收标准**：
- `cargo check` 通过
- `cargo test -p aether-engine` 中现有 lighting tests 仍通过
- 运行 Launcher，画面应明显过曝（因为缺少 tone mapping）

---

## 任务 3：CompositePass 改为 write PostProcessInput

- [ ] 在 `resource.rs` 新增 `PostProcessInput` tag
- [ ] 修改 CompositePass `signature()`：添加 `.write::<PostProcessInput>("post_process_input", Rgba16Float)`
- [ ] 修改 CompositePass `execute()`：render pass 输出到 `PostProcessInput` 而非 `surface_view`
- [ ] 更新 CompositePass 单元测试

**验收标准**：
- `cargo check` 通过
- `cargo test` 中 composite signature test 更新并通过
- PipelineBuilder 能正确构建（无 missing producer 错误）

---

## 任务 4：ToneMappingPass 实现（shader + pass 结构）

- [ ] 创建 `renderer/passes/tonemapping.rs`
- [ ] 实现 `Pass` trait：signature / init / resolve / execute
- [ ] 内联 WGSL shader：全屏 quad + ACES / Reinhard / Off 三种算法
- [ ] Uniform buffer：`algorithm: u32`
- [ ] 添加单元测试：signature / init / resolve

**Shader 伪代码**：
```wgsl
fn reinhard(x: vec3<f32>) -> vec3<f32> { return x / (x + 1.0); }
fn aces(x: vec3<f32>) -> vec3<f32> { /* fitted */ }

@fragment
fn fs_main(in: VertexOutput) -> @location(0) vec4<f32> {
    let hdr = textureSample(input_tex, sampler, in.uv).rgb;
    var ldr: vec3<f32>;
    switch(uniforms.algorithm) {
        case 0u: { ldr = clamp(hdr, vec3(0.0), vec3(1.0)); }
        case 1u: { ldr = reinhard(hdr); }
        case 2u: { ldr = aces(hdr); }
        default: { ldr = hdr; }
    }
    return vec4<f32>(ldr, 1.0);
}
```

**验收标准**：
- `cargo check` 通过
- `cargo test` 中 tonemapping tests 通过
- Pass signature 测试：reads PostProcessInput，writes Swapchain

---

## 任务 5：Launcher 集成（PipelineBuilder 注册 + UI）

- [ ] `passes/mod.rs` 导出 `tonemapping`
- [ ] `main.rs` PipelineBuilder 中 `.add(ToneMappingPass::init(&ctx.device))`
- [ ] `scheduler.rs` 新增 `set_tone_mapping_algorithm()` setter
- [ ] Launcher egui 面板添加算法下拉框（Off / Reinhard / ACES）
- [ ] 默认算法设为 ACES

**验收标准**：
- `cargo check` 通过
- Launcher 启动无 panic
- UI 下拉框可切换算法，画面实时更新

---

## 任务 6：编译验证

- [ ] `cargo check --all-targets` 0 errors
- [ ] `cargo clippy --all-targets` 无新增 warnings
- [ ] `cargo test -p aether-engine` 全部通过

**验收标准**：
- `cargo check` 0 errors, 0 warnings
- `cargo clippy` 干净
- 所有测试绿色

---

## 任务 7：运行时验证 + 视觉测试

- [ ] 运行 Launcher，加载 `08_tonemapping.ron`
- [ ] 分别切换 ACES / Reinhard / Off，确认视觉效果差异
- [ ] ACES：暗部有细节，高光不爆
- [ ] Reinhard：与修改前基线一致（regression 验证）
- [ ] Off：明显过曝/裁切，用于 HDR 调试
- [ ] Debug grid/gizmo 仍正确显示
- [ ] 运行 `scripts/should-verify-visual.py` 确认 MUST_VERIFY
- [ ] Agent 读取截图，判定 PASS/FAIL
- [ ] 生成视觉测试报告 `tests/reports/YYYY-MM-DD-tonemapping.md`

**验收标准**：
- 三种算法视觉差异可感知
- ACES 暗部细节优于 Reinhard
- DebugLinePass 叠加正确
- 视觉测试报告已归档
- Issue 可关闭
