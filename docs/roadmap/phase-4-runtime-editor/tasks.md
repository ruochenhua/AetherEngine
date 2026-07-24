# Phase 4 执行任务

## E4.1 状态拆分

先为 `App` 当前字段建立所有权表，再一次只移动一个子系统。每次移动后运行 Launcher release build 和对应场景；禁止在一次提交中同时改变 App 状态、Render Graph 和 shader 行为。

## E4.2 AsyncAssetLoader 集成

为加载请求定义稳定 handle 和状态查询 API；场景 loader 只产生请求，AssetRuntime 负责后台 IO，RenderRuntime 在 prepare 阶段提交 GPU upload。失败状态必须携带 path 和错误链。

## E4.3 材质

先实现默认 PBR 材质和纹理 fallback，再扩展 shader permutation。材质 GPU bind group 更新使用 wholesale replacement；场景切换清除 GPU 材质缓存。

## E4.4 编辑器可靠性

为保存失败、撤销/重做、无效资产引用和场景切换分别增加测试；UI 只调用 domain command，不直接操作 renderer resource table。

## 验证

```text
cargo test -p aether-engine --lib
cargo build -p aether-launcher --release
```
