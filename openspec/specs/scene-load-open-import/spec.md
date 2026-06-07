# Scene Load — Open/Import Semantics + Camera/Light ECS Specification

## Purpose

重构 `SceneLoader` 的加载逻辑，区分 `Open`（替换）和 `Import`（追加）语义，同时将相机和灯光作为 ECS 实体 spawn，为完整的场景 save/load 往返奠定基础。

## Requirements

### Requirement: Open scene (replace semantics)

The system SHALL provide `SceneLoader::open_scene()` which clears the world and fully loads a scene.

#### Scenario: Opening a scene file
- **WHEN** `open_scene(path, device, registry, world)` is called
- **THEN** the world is cleared of all existing entities
- **AND** a `(Transform, Camera)` entity is spawned from the RON camera config
- **AND** a `(Transform, Light)` entity is spawned from the RON light config
- **AND** object entities `(Transform, MeshHandle, MaterialUniform, Visibility, Name)` are spawned
- **AND** `LightingUniforms` are returned

### Requirement: Import scene (append semantics)

The system SHALL provide `SceneLoader::import_scene()` which appends only objects to the existing world.

#### Scenario: Importing objects
- **WHEN** `import_scene(path, device, registry, world)` is called
- **THEN** only object entities are spawned (no camera, no light)
- **AND** existing camera, lights, and objects are preserved
- **AND** `LightingUniforms` are returned (from the imported scene's config)

### Requirement: Camera entity spawn during build

The system SHALL spawn a camera ECS entity when loading a scene.

#### Scenario: Camera round-trip
- **WHEN** `build_world()` processes a `SceneDescription`
- **THEN** it spawns `(Transform, Camera)` from `desc.camera`
- **AND** `Transform` holds position, yaw, pitch as quaternion
- **AND** `Camera` holds fov (from config)

### Requirement: Light entity spawn during build

The system SHALL spawn a light ECS entity when loading a scene.

#### Scenario: Light round-trip
- **WHEN** `build_world()` processes a `SceneDescription` with lights
- **THEN** it spawns `(Transform, Light)` from `desc.lights[0]`
- **AND** `Transform` holds direction as quaternion (for directional lights)
- **AND** `Light` holds light_type, color, intensity, cast_shadow

### Requirement: Object name persistence

The system SHALL attach `Name` components to object entities during load.

#### Scenario: Named object spawn
- **WHEN** `build_world()` processes an object with `name: "MyCube"`
- **THEN** the spawned entity includes `Name("MyCube".into())`

## Design Decisions

### D1: world.clear() for open_scene

`open_scene` uses `world.clear()` (hecs built-in) to remove all entities. This is simpler and more reliable than iterating and despawning individually.

### D2: import_scene preserves camera/light

`import_scene` intentionally does NOT spawn camera or light entities. This lets users build composite scenes by importing multiple object sets while keeping their current camera position and lighting setup.

### D3: Single light only

For now, only the first light in `desc.lights` is spawned. Multi-light support is a separate future issue.

## Impact Analysis

| 文件 | 变更类型 | 说明 |
|------|---------|------|
| `scene/loader.rs` | 修改 | `build_world` 拆分相机/灯光/物体 spawn；新增 `open_scene` |
| `scene/loader.rs` | 修改 | `import_scene` 改为只调用物体 spawn |
| `scene/mod.rs` | 可能修改 | 导出新 API |
| `ecs/components.rs` | 无修改 | 复用 #61 的 `Light` 和 `Name` |

## Acceptance Criteria

- [ ] `open_scene` 后 world 中只有新场景的实体，无残留
- [ ] `import_scene` 后 world 保留旧实体并追加新物体
- [ ] `open_scene` 后查询到 `(Transform, Camera)` 实体
- [ ] `open_scene` 后查询到 `(Transform, Light)` 实体
- [ ] `open_scene` 后物体带有 `Name` Component
- [ ] 所有现有测试继续通过
- [ ] `cargo clippy` 无新增警告
