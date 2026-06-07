# Scene ECS Components Specification

## Purpose

为支持场景 save/load 的完整往返，在 ECS 中引入两个新的 Component：`Light`（灯光）和 `Name`（实例名称）。这是场景序列化基础设施的基础层，使灯光和物体名称成为可被查询、编辑、持久化的一等 ECS 实体数据。

## Requirements

### Requirement: Light Component

The system SHALL define a `Light` ECS Component that stores light properties for directional/point/spot lights.

#### Scenario: Light entity spawn
- **WHEN** `build_world` loads a scene with lights from RON
- **THEN** each light is spawned as an ECS entity with `(Transform, Light)`
- **AND** `Light` stores `light_type`, `color`, and `intensity`
- **AND** `Light` is serializable via `serde` for RON round-tripping

#### Scenario: Light entity query
- **WHEN** the renderer or serializer queries the world for lights
- **THEN** `world.query::<(&Transform, &Light)>()` returns all light entities
- **AND** the first directional light can be extracted for `LightingUniforms`

### Requirement: Name Component

The system SHALL define a `Name` ECS Component that stores a human-readable instance name for each object entity.

#### Scenario: Named object spawn
- **WHEN** `build_world` loads a scene where an object has `name: "MyCube"`
- **THEN** the spawned entity includes `Name("MyCube".into())`
- **AND** `MeshHandle.name` continues to store the mesh reference (e.g. "cube")

#### Scenario: Name extraction during save
- **WHEN** `serialize_world` traverses object entities
- **THEN** it reads `Name` to populate `ObjectConfig.name`
- **AND** unnamed objects default to empty string

### Requirement: Component registration

The system SHALL ensure `Light` and `Name` are accessible from all modules that need them.

#### Scenario: Module imports
- **WHEN** `scene/loader.rs` or `scene/serializer.rs` uses `Light` or `Name`
- **THEN** the components are importable from `aether_engine::ecs::components`

## Design Decisions

### D1: LightType reuse

`Light` uses the existing `LightType` enum from `renderer::light`. This enum already derives `Serialize`/`Deserialize` and covers Directional/Point/Spot. No new enum needed.

### D2: Name is not serialized via serde

`Name` does not derive `Serialize`/`Deserialize`. It is an ECS-only runtime component. Serialization goes through `ObjectConfig.name` ↔ `Name` manual mapping in loader/serializer. This keeps ECS components decoupled from RON format.

### D3: Single light limitation (for now)

This spec intentionally does NOT add multi-light support to `LightingUniforms` or the shader. The renderer still consumes only the first directional light. Multi-light rendering is tracked as a separate future issue.

## Impact Analysis

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `ecs/components.rs` | 新增 | 添加 `Light` 和 `Name` struct |
| `scene/loader.rs` | 修改 | `build_world` 为灯光 spawn `(Transform, Light)`，为物体附加 `Name` |
| `scene/serializer.rs` | 修改 | `serialize_world` 查询 `Name` 和 `Light` |
| `ecs/mod.rs` | 可能修改 | 导出新组件（如需要）|

## Dependency Compatibility Matrix

| Crate | 变更 | 风险 | 验证 |
|-------|------|------|------|
| hecs | 无（继续使用 `Component` derive） | 无 | `cargo check` |
| serde | `Light` 新增 derive | 无（`LightType` 已有 derive） | `cargo check` |
| glam | 无 | 无 | — |

## Acceptance Criteria

- [x] `Light` Component 可 spawn、可查询、可序列化
- [x] `Name` Component 可 spawn、可查询
- [x] `build_world` 为灯光 spawn `(Transform, Light)`
- [x] `build_world` 为物体附加 `Name`
- [x] `serialize_world` 能读取 `Name` 和 `Light`
- [x] `cargo test` 通过
- [x] `cargo clippy` 无警告
