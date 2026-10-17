# EW5-A 实施报告：类型化依赖分析与只读引用预览

日期：2026-10-17
状态：已完成并通过独立任务审查

## 交付范围

- `project_dependencies` 服务扫描 Scene、Prefab、MaterialAsset 中受支持的类型化引用，保留字段种类、稳定来源定位、重复边、原始路径和规范候选路径。
- 缺失资源、未知/损坏文档、symlink、越界路径、未覆盖的外部引用格式、预算溢出和分析期间变化会阻止完整性结论。
- 文件和目录库存使用有预算的完整内容指纹；发布结果前重新比较库存，以识别同尺寸内容变化、目标替换、文件/目录增删。
- Project 面板提供 Show References，只读显示 Used by / Depends on、来源字段、解析状态和阻断诊断。目标固定；选择引用行只导航 Project；重新检查指纹和重新分析在后台运行；Escape 仅在引用面板内部控件拥有焦点时关闭，不抢占搜索或名称输入。
- 没有开放 Move、Rename 或 Delete。读/关闭预览不改变磁盘、World、AssetManager、Project 选择、索引版本、实体选择或撤销/重做状态。

## 交互与安全边界

只读引用预览不能证明资源可以安全移动或删除。当前仍缺少通用场景脏状态和运行时路径迁移/重键机制；World、AssetManager、材质/纹理缓存及 watcher 可能保留旧路径。因此下一阶段先关闭这些前置缺口，再设计单资源移动/重命名；删除排在后续。

## 验证

- 依赖分析：23 个集成测试通过，覆盖类型化引用、材质槽、Prefab、缺失/损坏引用、未知/未声明 GLTF 扩展、symlink、预算、指纹和扫描期变化。
- Project 面板：最终 12 个引用交互测试通过，含固定目标、异步过期检查、读/关状态不变、行导航和 5 个 Tab 聚焦控件的 Escape 回归；搜索框和新建文件夹输入不被 Escape 劫持。
- `./scripts/verify-module-health.sh` 通过，400 个 Rust 文件中无超限文件；未修改基线。
- `./scripts/verify-ci.sh` 通过，包括 workspace 测试、Clippy、格式、脚本检查和 workspace Release 构建。
- Release Launcher 的 clouds 场景 smoke check 通过。此切片未改渲染管线。

## 提交

- `cfeca43` `feat(editor): analyze typed authored resource dependencies`
- `1433e8c` `fix(editor): block unvisited glTF extension dependencies`
- `5596db3` `fix(editor): inspect undeclared glTF extension payloads`
- `aec9440` `feat(editor): preview pinned project resource references`
- `d7ac98f` `fix(editor): close references from keyboard-focused controls`

所有实现与修复均通过独立任务审查；两项重要问题（未覆盖 GLTF 扩展、键盘焦点内 Escape）按测试先行修复后复审通过。
