# 下一阶段：项目资源浏览器与日志面板

状态：规划草案 v0.1 · 2026-10-11  
目标：让 Aether Engine 从“能打开和编辑场景”迈向“能在引擎内管理项目内容并解释运行问题”的完整创作工作流。

## 产品目标

围绕一个场景创作流程交付下一阶段：用户能在项目中找到并检查场景、模型、纹理、材质和 Prefab；把资源放入场景或分配给对象；运行时能在编辑器中读懂引擎发生了什么，并定位资源加载、场景操作和渲染问题。

项目资源浏览器与日志面板是两个相互配合的面板：资源浏览器负责内容的发现和使用，日志面板负责反馈内容加载和编辑器操作的结果。二者都要留在当前编辑器工作区里，并为后续完整场景创作打基础。

## 当前基础

- Launcher 有示例场景列表、Scene Hierarchy、Inspector、视口和 RON 场景打开/保存操作；当前 UI 使用 egui 的左右面板，没有完整的可停靠窗口系统。
- AssetManager/typed AssetStore 已使用项目根目录相对路径，支持材质、纹理和 Prefab 热重载；持久 AssetId 由 `AssetKind + canonical relative path` 组成，路径更改会改变 ID。模型加载器支持 OBJ 与 glTF。
- Asset path 的词法规范化不等于文件系统 containment；资源浏览器不得跟随 symlink 到项目根目录之外。文件操作要在确认真实父目录和目标路径后才能开放。
- 日志使用 `tracing`，目前由 `tracing_subscriber::fmt()` 输出到终端。编辑器事务另有场景旁的 `.editor.jsonl` 操作记录，它与运行诊断日志用途不同，应保留为独立记录。
- 场景和资产位于 `scenes/` 与 `assets/` 等项目目录。路径移动会影响 RON 中的资源引用，因此文件操作必须理解依赖关系。

## 参考设计与取舍

参考现有引擎的常用工作流，不追求复制其全部功能：

- Unreal Content Browser 将导航/来源、搜索、类型筛选、资源视图和导入/创建操作放在同一套浏览流程中；资源可拖入关卡。Aether 的第一版采用这些高频交互，不做 Collections、复杂自定义过滤器或多浏览器窗口。
- Godot 将项目文件系统作为引擎识别的资源目录，文件扫描、资源类型和导入状态由编辑器管理；资源可从 FileSystem 拖到 Inspector 属性。Aether 应以项目资源为中心，避免把 `.git`、构建产物和验证报告伪装成可导入资产。
- Unreal Output Log 位于编辑器工作区，可查看运行日志，日志也写入项目日志目录；按类别和等级组织有助于过滤。Aether 第一版做结构化查看、过滤和保存，不提供任意控制台命令执行。

官方参考：

- [Unreal Content Browser 界面](https://dev.epicgames.com/documentation/unreal-engine/content-browser-interface-in-unreal-engine?lang=en-US)
- [Unreal Content Browser 的筛选与 Collections](https://dev.epicgames.com/documentation/unreal-engine/filters-and-collections-in-unreal-engine?lang=en-US)
- [Unreal 资源导入](https://dev.epicgames.com/documentation/unreal-engine/importing-assets-directly-into-unreal-engine?lang=en-US)
- [Unreal Logging 与 Output Log](https://dev.epicgames.com/documentation/unreal-engine/logging-in-unreal-engine?lang=en-US)
- [Godot EditorFileSystem](https://docs.godotengine.org/en/stable/classes/class_editorfilesystem.html)
- [Godot Inspector Dock 与 FileSystem 资源交互](https://docs.godotengine.org/en/stable/tutorials/editor/inspector_dock.html)

## 体验草图

编辑器保留顶部菜单、中央 3D 视口、左侧 Scene、右侧 Inspector。新增底部可调整高度的工作区，含 `Project` 和 `Log` 页签；两个页签都要从启动 Menu 和 Running 工作区访问，以便用户能找到项目内容，也能看到启动阶段的诊断：

- `Project`：目录树、面包屑和路径前进/后退；名称搜索、资源类型过滤、可读扫描状态；主体先使用稳定列表视图。选中资源后显示预览、路径、类型、大小、索引/加载状态和依赖摘要。首版不做收藏、Collections、可自定义过滤器或双视图模式。
- `Log`：带时间、等级、目标/类别、消息和展开字段的事件列表；可按等级、类别和文本过滤，暂停自动滚动、清空视图、复制选中项。结构化路径允许跳转到 Project；无路径上下文时显示完整事件详情。
- 右侧 Inspector 可切换查看资源详情，同时保留 Scene 中当前对象选择；退出资源详情后回到对象 Inspector，不能意外改写场景选择。
- 初期只需固定布局和可调分隔条，不要求像 UE 那样任意拆分、漂浮或保存多套工作区布局。
- 窗口键盘快捷键只有在 UI 未占有键盘焦点时才作用于场景；Project 搜索、文件重命名等控件不能触发实体删除等场景快捷键。

## 执行切片

### EW0 · 工作区面板骨架

任务包：[EW0 实施简报](ew0-workspace-brief.md)

- 增加底部 `Project` / `Log` 页签、可调整高度与显隐入口；两个页签都可从 Menu 和 Running 两种状态打开；保持视口、Scene、Inspector 的现有交互可用。
- 保存用户选择的页签、面板开关和尺寸；全屏视口时隐藏工具面板并能恢复。
- 建立明确的 scene selection / asset selection 状态；查看资源不清除当前实体选择。
- 只有 UI 未占键盘焦点时才执行场景快捷键；用 egui 输入事件测试搜索框内的 Delete/快捷键不会修改场景。
- 提供 1280×720 和更小尺寸布局检查，避免底部面板压缩后视口和 Inspector 无法操作。

### EW1 · 结构化日志采集与 Log 面板

任务包：[EW1 实施简报](ew1-log-panel-brief.md)

- 捕获引擎和 Launcher 的 `tracing` 事件，显示等级、时间、target/category、消息及结构化字段。
- 提供等级开关、类别过滤、全文搜索、暂停滚动、清空视图、复制消息和错误/警告计数。
- UI 与 FileSink 都使用有界队列，采集不能等待 UI 或磁盘；UI 和文件队列分别显示按等级累计的丢弃数与 sink 健康状态。正常 I/O 时事件被持久化，队列满/写失败时不承诺每条 ERROR 必然落盘。
- 将日志写入 `.aether/logs/` 或明确配置的位置，该目录被 Git 忽略；支持单次启动标识和大小/保留策略。编辑操作 `.editor.jsonl` 保持独立。
- 明确失败语义：事件采集不等待磁盘；任一队列满时该事件可被丢弃，按等级累计并标记 sink degraded；文件 writer 检测到 I/O 错误后只向 stderr 写一次故障摘要并暴露健康状态，禁止在日志回调中递归记录。正常关闭时 flush 所有已接收事件。进程被强杀时，最近一段异步日志可能尚未落盘。
- 分离文件 sink 与 UI sink。UI Clear 只清当前视图、Pause 只停自动滚动、Filter 只控制显示，均不停止底层采集或删除持久日志。
- 为关键错误提供可点击上下文：资源路径能跳转到 Project 并选中资源；场景加载错误能打开对应场景或定位错误详情。

### EW2 · 项目资源索引与浏览

任务包：[EW2 实施简报](ew2-project-browser-brief.md)

- 从当前项目根目录扫描固定的 `scenes/`、`assets/` 内容根；异步扫描并显示扫描、刷新和错误状态。MVP 不允许配置任意绝对目录。
- 显示目录树、面包屑、路径历史、文件类型图标、名称搜索、类型过滤、稳定排序和单一列表视图；空目录和无匹配结果有明确空态。
- 初始识别 RON 场景、Prefab、MaterialAsset、glTF、OBJ、常见纹理，以及未知文件；RON 类型必须通过受控目录与严格 schema/结构检查共同判定，不因 serde 默认字段而把任意 RON 误认成材质。
- `scenes/` 或 `assets/` 根本身若是 symlink，则整根拒绝并显示错误；其子目录/文件 symlink 不遍历。canonical path 仍越过项目根的条目拒绝。排除 `.git`、隐藏目录、编译输出、报告产物和缓存。
- 选中项显示路径、大小、资源类别、可读预览/元数据和错误状态；双击 RON 场景打开，双击其余已支持类型打开预览或 Inspector。
- 刷新或外部文件变化后生成新快照；旧扫描结果不能覆盖较新的扫描。扫描失败保留上一份有效列表，并显示诊断。
- 资源索引是项目文件视图，不主动加载整个项目到 GPU；显式打开/预览后仍沿用 AssetManager/AssetStore。

### EW3 · 资源预览与拖放

任务包：[EW3 拖放实施简报](ew3-drag-drop-brief.md)

状态（2026-10-13）：EW3 首个切片已实现并通过自动验证，范围为 Project 单实体 OBJ 拖入视口、y=0 地面放置、场景/操作日志持久化、Undo/Redo、保存重开和基线日志 replay 解析。文件资源浏览器的 Folders/Resources 分割条已改用 egui 原生可调整面板，并在 Release Launcher 中确认可拖动。MaterialAsset 与 inline texture 拖放、缩略图仍是后续切片。

- 规划审阅结论：先交付 Project 到视口的单实体 OBJ 地面放置，再扩充材质/纹理属性拖放；现有 renderer 用全窗投影，egui rect 只负责限定 drop 命中区。
- 实现顺序固定为 EW3-A 放置数学、EW3-B 拖放路由、EW3-C1 Engine operation/history、EW3-C2 Launcher host resolver、EW3-C3 Project/viewport 集成，每个切片先 RED 再 GREEN。

- 拖放前明确 Scene 实体选择与 Project 资源选择如何共存，单击预览、双击打开、拖拽起点和取消行为各自清晰。
- EW3 分阶段交付：第一切片做单实体 OBJ 地面放置、保存/重开与 Undo/Redo；后续切片加入 MaterialAsset 和 inline texture 槽赋值。模型拖入视口以全窗 render viewport rect 建射线、egui viewport rect 限定 drop 区，并使用 y=0 地面放置规则；无有效落点时拒绝且无副作用。
- 后续 EW3 切片再交付 MaterialAsset 拖入 Mesh 的 `material_asset` 链接字段和纹理拖到 inline Material 的 Albedo/Normal/ORM/Emissive 对应字段。字段不兼容或场景对象仍链接只读材质时显示可读拒绝原因。
- 资源赋值作为 EditorOperation，支持撤销/重做和加载失败回滚。禁止通过拖放隐式 detach linked MaterialAsset。
- 第一切片显示 Project 当前支持的类型/路径等资源摘要；模型缩略图、纹理缩略图和尺寸属于后续 EW3，当前不阻挡单实体放置。RON 资源显示类型化摘要；不支持预览的资源显示清晰状态。
- 拖放前校验资源类型、真实路径 containment 和目标字段；拒绝时给出可读反馈，不留下半成品实体或引用。

### EW4 · 导入与安全文件操作

- 先支持新建项目子文件夹、导入单纹理和自包含 GLB；对依赖外部 MTL/纹理/buffer 的 OBJ/glTF 拒绝导入并列明依赖，不复制不完整资源包。
- 导入使用项目内 staging 目录，验证格式和全部依赖后再发布；失败清理 staging 且不改变项目索引。
- 新建文件夹与单资源复制可先交付；删除前检查引用，未知/损坏的依赖文档存在时保守拒绝。

### EW5 · 依赖感知的移动、重命名与删除

- 建立类型化依赖 visitor，覆盖 Scene 中 MeshRef/MaterialAsset/Prefab、Prefab 中 Mesh/Material 与 override、MaterialAsset 中所有纹理槽、Terrain heightmap/layer textures。
- 禁止字符串全局替换。移动/重命名仅对已支持 schema 执行；无法解析文件、未知引用或真实路径检查失败时拒绝操作并列明原因。
- 检查脏场景和运行时引用：要求先保存或取消；提交文件操作后同步更新索引、热重载 watcher 与当前 World 引用。
- 多文件改写使用 staging + 原子替换/回滚流程；明确跨文件系统移动策略。文件操作撤销需要独立的事务合同，不复用仅能记录实体操作的 EditorOperation。
- 被引用资源不能直接删除；列出依赖方，并给出先替换引用或取消的路径。

## 顺序与依赖

```text
EW0 工作区面板骨架
 ├─ EW1 日志采集与面板
 └─ EW2 项目资源索引与浏览
      └─ EW3 资源预览与拖放
           └─ EW4 导入与基础文件操作
                └─ EW5 依赖感知的移动/重命名/删除
```

**阶段进度（2026-10-13）：EW0–EW2 已交付；EW3 首个 OBJ 放置切片已交付。** Project 目录树与资源列表分割条现已可拖动调整；EW3 后续仍需实现材质/纹理槽拖放并完成该部分 GUI 体验确认。EW4/EW5 仍需先完成导入依赖、类型化引用 visitor 和跨文件事务设计，再开放会改变项目文件的操作。

## 阶段验收

第一批验收（EW0–EW2）：

1. 用户从启动菜单和 Running 工作区都能打开 Project；无需终端即可浏览 `scenes/` 与 `assets/`、搜索并按类型筛选。
2. 可从 Project 打开场景；资源选择可显示详情且不丢失 Scene 对象选择。
3. 从启动 Menu 和 Running 都能打开 Log，包括启动/场景加载失败时的诊断。Error/Warning 正常情况下写入有界 FileSink 并可检索；队列溢出和 I/O 错误按等级计数并显示 sink degraded，不承诺故障条件下无损。
4. 外部新增、修改、删除和扫描错误之后，Project 状态可刷新且不崩溃；损坏/不支持的资源有明确状态，symlink 不越界。
5. 输入框拥有键盘焦点时，场景快捷键不会误触发。
6. 交付 Project scanner、日志队列/过滤和 UI 状态的测试；Release Launcher 手动体验记录；遵守根目录 `AGENTS.md` 的完整 CI 与专项运行要求。

最终阶段验收追加 EW3–EW5：拖放撤销/重做、依赖完整导入，以及引用安全的文件操作。

## TDD 验收合同（第一批）

每个条目先新增能表达用户行为的测试，单独运行并确认按预期失败，再写最少生产代码使其通过。若失败源于测试编译错误或错误夹具，应先修测试；不得先完成 UI/服务实现再补测试。

### EW0 · 面板与输入路由

- 测试 Menu 与 Running 均能切换 Project/Log 工作区；失败基线应是当前 UI 无此页签/面板。
- 测试 Menu 状态出现启动/场景加载错误时可打开 Log 并读到该事件。
- 测试打开 Project 资源详情后原 Scene entity selection 保留，关闭详情后 Inspector 回到原 entity。
- 用 egui 键盘输入事件验证搜索框获得焦点时 `Delete` 不发出 despawn 操作；失焦时现有场景快捷键行为保持。
- 测试全屏开关隐藏并恢复两侧和底部面板，面板状态跨帧保持。

### EW1 · LogStore 与 tracing sinks

- 真实 `tracing` subscriber 作用域采集结构化事件，断言 level、target、消息、字段和时间字段保真。
- 多线程并发事件可被 UI 安全 drain；缓冲容量固定，溢出计数准确增加。
- level/category/text filter 只影响查询结果；Clear 只清 UI store；Pause 仅停自动滚动，事件仍被采集并写文件。
- 有界持久 sink 可用临时目录读回接收的 ERROR 记录；队列满时按等级计数丢弃，目录不可写时返回/展示 sink 状态并由 writer 执行一次 stderr 兜底，不递归日志。
- 正常 shutdown 后所有已接收且入队的事件可读；队列容量、文件大小/轮转边界保持配置上限。无 GUI 模式下仍能写文件。

### EW2 · ProjectScanner 与 UI 查询

- 临时项目夹具测试资源根 allowlist、确定性排序、过滤、搜索和路径历史。
- `scenes/` 或 `assets/` 根本身是 symlink 时整根拒绝；子目录/文件 symlink、隐藏目录、`target/`、`.git/`、`tests/reports/` 不进入结果；词法与 canonical path 越界都被拒绝。
- 严格 scene/prefab/material RON fixture 能区分资源类型；未知、歧义、损坏的 RON 保留为 Unknown/Invalid，不因 serde 默认值误分类。
- 空 MaterialConfig RON 与其他默认字段齐全的无关 RON 都不能被归为 MaterialAsset；目录与必需识别字段必须同时满足。
- 初次 scan、刷新后新增/修改/删除、扫描中断/失败有明确状态；失败保留最后有效快照，迟到的旧 scan 不能覆盖新结果。
- 从列表打开有效场景后选中场景正确；无效场景打开失败时当前运行场景保持不变并产生可检索日志。

### TDD 执行记录

第一批实现报告必须逐项记录 RED 命令与预期失败、GREEN 命令与通过结果。测试需使用真实 temp-dir 文件系统、真实 tracing event capture 与 egui 输入；只有操作系统文件权限等不可控边界允许做窄接口替身。

## 决策地图

以下问题会改变后续实现边界，先通过研究或小型原型收敛。推荐基线已足以启动 EW0–EW2。

### #1 面板布局与交互

Blocked by: 无  
Type: Prototype

**问题：** 底部 `Project` / `Log` 页签如何与启动菜单、Running、全屏视口及键盘快捷键共存？EW3 的 viewport rect 事件路由也依赖此合同。

**推荐基线：** 固定的底部 Tab + 可调高度，保留左 Scene、右 Inspector；首版不建通用 docking system。

### #2 项目资源边界与扫描

Blocked by: #1  
Type: Research

**问题：** 当前阶段固定扫描 `scenes/` 与 `assets/` 是否足以覆盖用户内容？后续显式项目清单怎样声明自定义资源根、忽略规则和日志目录？

**推荐基线：** 第一版支持当前项目根目录和明确目录 allowlist；显式项目创建/切换可以进入后续阶段。

### #3 资产引用身份与重命名策略

Blocked by: #2  
Type: Research

**问题：** 当前 AssetId 是路径身份且运行时缓存按该键索引。移动/重命名时，如何同时处理场景配置、Prefab、材质相对纹理路径、热重载 watcher 和运行中 World？

**推荐基线：** 先实现可审计的类型化依赖索引并暂不移动已引用资源；完整 visitor 覆盖已知字段之后再开放受限改写。未知/不可解析引用存在时阻止操作。稳定 ID 留待实证路径身份造成不可接受的运行时复杂度后再决策。

### #4 日志持久化与线程安全

Blocked by: #1  
Type: Prototype

**问题：** `tracing` Layer 如何把多线程事件安全地交给 UI，同时写持久文件并限制内存和磁盘增长？当 UI 队列溢出、磁盘满、权限拒绝或进程被强杀时，如何报告损失？

**推荐基线：** FileSink 与 UiSink 分离；事件采集不等待磁盘，UI 使用有界队列/环形缓冲并报告丢弃数，文件 writer 异步 flush/轮转。写失败只做一次 stderr 兜底并暴露 sink 健康状态；不承诺强杀时最后一批日志必定落盘。日志回调不再调用 tracing。

## 进入实现前的门槛

- EW0–EW2 的基线合同已在本计划与各实施简报中冻结，可先进入 TDD 实施；#1 面板交互和 #4 logging prototype 纳入对应任务的第一组测试先行任务。
- EW2 第一版固定 `scenes/` 与 `assets/`，按 schema/目录严格分类，真实路径只限项目内且跳过 symlink。
- EW3–EW5 的文件写操作必须等待 #2/#3 和 viewport drop 原型结论；不允许在首批实现里偷带移动、删除或未定义的材质链接行为。
- 将 EW0–EW2 写成独立任务包并指定 primary test，再开始第一批实现；后续阶段任务包需包含文件依赖和回滚合同。
