# EW3 · 资源预览与拖放实施简报

状态：实施合同 v0.1 · 2026-10-11

## 用户结果

用户可以从 Project 拖入一个受支持的单网格模型，在视口可见位置放到 y=0 地面；模型会被选中，Undo/Redo 能完整撤销和恢复，操作成功后场景 RON 与 `.editor.jsonl` 一起持久化，重开仍存在。面板资源单击保持当前 Project selection/details 预览，双击保持打开场景的现有行为；取消拖拽不改变场景。完整 EW3 后续切片还支持 MaterialAsset 与 inline texture 槽赋值。

## 现状与约束

- 3D 内容当前按完整窗口的 RenderContext 尺寸和相机宽高比渲染；egui CentralPanel 透明覆盖在其上，并不决定投影尺寸。
- winit 鼠标坐标为物理像素，egui 矩形为 point。拖放命中需用 scale factor 转成同一物理像素空间；射线仍以完整窗口渲染矩形为坐标基准。不能把光标重新归一化到 CentralPanel 后再使用全窗投影。
- 当前相机只有透视 FlyCamera。复用 `renderer::picking::screen_ray`，但新增有界输入验证与 y=0 平面相交；近平行、非有限、零尺寸、窗口外、相机背后均拒绝。
- Project scanner 已验证资产位于项目根内且不跟随 symlink。拖放时仍须重新确认 canonical containment，因为扫描后文件可能变化。
- 现有 SceneLoader 和 OBJ loader 可加载文件网格；多子网格会生成多个 ECS entity。首个切片只接受生成单一渲染实体的 OBJ，GLTF、多子网格 OBJ 和含外部依赖的资源明确拒绝并解释原因。
- EditorHistory 的 `EditorContext` 不持有资源加载器；不要把 GPU 加载耦合进纯 EditorOperation。需要先冻结一条可测试的 host/engine seam，再实现操作。
- `RuntimeEntity::capture` 未记录 `MaterialConfig`、`ExtractedMaterial`、`MaterialAssetRef`、`MaterialAssetStatus`；在新放置操作依赖 Delete/Restore inverse 前，先用 RED/GREEN 修复运行时 inverse 的完整材质状态往返。mesh source 由 `MeshHandle.source` 表达，serialized object operation 必须携带等价 `MeshRef`。

## 冻结的交互合同

1. 只允许从 Project 的已识别资源项开始拖拽。资源单击表示选中并显示当前 Project details/Inspector 资源摘要，不承诺像素缩略图；双击场景仍打开场景。开始拖动后不改变当前 Scene entity selection。
2. viewport drop rect 来源于 egui central viewport 实际响应区域；drop 判定采用左闭右开边界 `[min, max)`，egui point 乘 `pixels_per_point` 变为物理像素，不取整。物理鼠标坐标命中 top/left edge，right/bottom edge 属于 viewport 外。
3. 只有释放点落在 viewport 的交互矩形内才接收 drop；其余面板、工具条、拖拽取消、鼠标释放在窗口外均为无操作。
4. 鼠标物理坐标以完整窗口渲染 rect `(0, 0, width, height)` 建射线。egui rect 仅做 drop 命中判断，射线仍按全窗投影求解。
5. 射线与水平 `y=0` 平面求交；只接收有限且 `t > 0` 的交点。不做任意相机前方距离 fallback，不做已有 mesh surface placement。模型默认 identity rotation/unit scale；平移 y 取 `-local_aabb.min.y`，让本地 AABB 底面落在 y=0。
6. 支持 OBJ 单渲染实体；判定要求 OBJ 不含 `mtllib` 指令且 loader 产出无 submesh 的单网格。拒绝 GLTF、OBJ 多 submesh、外部材质/纹理依赖、坏格式、缺失文件、symlink 越界、路径越界、加载错误。拒绝时显示 Project/Log 可读原因，不改变 World、选择、历史栈、场景文件和 JSONL。
7. 成功 drop 先验证、加载并解析材质，之后一次性提交：创建带 `Transform`、`MeshHandle.source` (`MeshRef`)、`MaterialConfig`、`MaterialUniform`、`ExtractedMaterial`、`Visibility`、`Name` 的实体，选中新实体，并登记单个可重放操作。operation JSONL 携带稳定实体名、MeshRef 和完整序列化 object/material config；mesh/material/texture 运行时句柄不写进 JSON。Launcher host 提供 asset resolver，首次 apply 与显式 operation-log replay 均按同一 resolver 从项目相对路径重建 runtime components。operation-log replay 只用于从基线场景重建/测试，不在日常 reopen 中运行。重开时已保存场景 RON 是权威状态，Launcher 不读日志恢复 undo 栈，Undo/Redo 不跨会话。
8. drop 事务是一次 `EditorHistory` commit：先 host preload，再变更 World，后持久化场景 RON 与 operation JSONL，只有两文件都成功后才提交 history 并更换 selection。可观测的写入失败会尽力恢复旧文件字节及 World/history/selection；若文件回滚本身失败，必须返回包含原始写入错误和 rollback 错误的明确诊断。该双文件更新不承诺进程崩溃/断电时的原子性。成功 drop 不要求用户再点 Save。后续显式 Save 失败时保留当前内存编辑和上一次成功保存文件，并显示错误，不回滚已经提交的 drop。
9. 选择规则：拖动和取消期间 selection 不变；成功 drop 选中新对象；Undo 删除它，若当前选择正是该对象则自然清空，若用户已选中其他实体则保留该选择；Undo 不恢复 drop 前选择；Redo 重新选中放置对象。apply/undo/redo 持久化失败不改变选择。选择仍为瞬时 UI 状态，不写入场景 RON 或 operation JSONL。
10. 本首个交付不做 MaterialAsset 或 inline texture 属性槽拖放、像素缩略图、GLTF/group placement、mesh surface snapping。MaterialAsset/texture assignment 是完整 EW3 的后续切片，需复用预加载、可重放操作和完整 Undo 状态合同。

## 实施任务顺序

### EW3-A · 视口与地面放置纯逻辑

- 测试全窗物理像素射线与 viewport drop rect 的坐标换算、包含边界、零尺寸和无效值。
- 测试地面交点、平行射线、朝上射线、`t <= 0` 背后交点和非有限输入拒绝；viewport 尺寸非正时拒绝，但合法负 world x/z 和向下 ray direction 必须允许。
- 实现纯 helper；生产代码不读取 UI/World。

### EW3-B · Project 拖拽路由

- 用 egui 原生拖拽 payload，从 Project 资源 row 启动；仅把有效释放点转为 pending placement request。
- 覆盖 panel/outside/cancel/unsupported resource 的无副作用行为，保持 click/double-click 语义。

### EW3-C1 · Engine object operation/history

- 先通过测试修复 editor inverse 对 mesh/material serialized state 的 round-trip。
- 冻结纯数据 `PlaceObject(ObjectConfig)` 与 runtime inverse 的边界；此切片只改 Engine 且通过 engine tests，不接 UI/渲染。

### EW3-C2 · Launcher host resolver/transaction

- 为 host preload/replay seam 先加 RED/GREEN。operation JSON 只保存 stable entity name、MeshRef、transform/material config；host 根据 operation 路径解析 containment、加载模型、解析材质并返回完整 runtime object。
- 资源解析失败不触及 World/history/operation log/selection；预加载成功后由 history 完成 World inverse 和 scene/JSONL commit。
- 新建或未保存场景必须先 Save As；没有 scene 和 `.editor.jsonl` 路径时显示可读拒绝原因且不改变状态。
- 首版 operation log 是当前已应用 editor operations 的审计/重建记录，不是跨会话 Undo 历史；Undo 会从 JSONL 移除撤销的操作。scene reopen 只加载已提交 RON、重置 history 且不读取 operation log。显式 replay 仅用于从日志对应的基线场景重建/测试，禁止对已经包含操作结果的 RON 再 replay。
- Scene RON 与 JSONL 分别原子替换。可观测的进程内写入错误触发 best-effort 回滚；回滚错误需与原始错误一起暴露。若文件恢复失败，内存 World/history/selection 仍回滚；将 editor 标为 fatal/degraded，拒绝后续编辑与写入，日志提示磁盘文件需要人工/外部恢复。C2 必须注入“第二文件写失败且第一文件恢复也失败”，验证内存回滚、写入被阻止及双错误诊断。当前不承诺 crash/断电原子性；为后续提高此保证需单独设计 journal recovery。
- 两文件成功后才更换 selection；drop 选中新对象，Undo 删除它；若当前选择是该对象则清空，若用户已选中别的实体则保留；Redo 重新选中放置对象；写入/回滚失败时 selection 保持原样。

### EW3-C3 · Project 到 viewport 集成

- 仅接 Project 拖拽 payload、viewport drop 请求和成功/错误 UI 状态，通过 C2 的 host resolver/transaction seam 发起操作；不重复实现资源验证/加载。
- 测试成功后 World/Selection/History 的一致变化、撤销/重做材质状态一致、RON 保存重开一致；Release GUI/no-GUI 使用 `scenes/18_file_mesh.ron` 与单对象 OBJ，另做真实 Project 拖放交互检查。

## 完成验收

- 每个任务先加行为测试，在旧代码/单独测试改动上确认预期 RED，再做最小实现并记录 GREEN 命令与结果。
- 拒绝路径完整断言 World、选择、undo/redo、操作日志和场景文件未变化。
- 成功路径验证场景序列化和 `SceneLoader::open_scene` 往返，及 Undo/Redo 运行时材质完整。
- `./scripts/verify-ci.sh`、`./scripts/verify-module-health.sh` 全通过；相关变化运行 Launcher release 并实际打开 18_file_mesh 场景；完成代码独立审阅。

## 待调查问题和门槛

- 拖放 payload 优先使用 egui 原生 payload；实现 EW3-B 前验证版本 API 支持跨 frame 生命周期和取消语义。
- `EditorHistory` 目前直接 apply/replay 到 World，host/runtime resource resolver seam 在 EW3-C2 冻结。预加载错误不触及历史；正常可观测写入错误且文件恢复成功时，World/history/log/selection 一致回滚。
- 若磁盘文件恢复本身失败，内存 World/history/selection 仍必须回滚；返回同时包含原始写入错误和恢复错误的 fatal/degraded 结果，标记项目文件状态需要恢复并阻止继续编辑/写入，直到重新加载或人工恢复。当前尚无崩溃恢复 journal，进程崩溃时不承诺跨文件一致性。
- 首版多子网格和任何 `mtllib` 资源明确拒绝；若这让现有主流 GLTF 样例无法使用，应另行设计父子 entity group operation，绝不悄悄丢子网格。
