# Scene Save/Load Launcher Integration Specification

## Purpose

将场景 save/load 完整接入 Launcher 编辑器，实现编辑器的闭环：菜单区分 Open/Import，Launcher 从 ECS 查询相机和灯光驱动渲染，保存时自动追加 `.ron` 扩展名。

## Requirements

### Requirement: Open Scene menu

The system SHALL provide an `Open Scene...` menu item that replaces all entities in the world.

#### Scenario: Opening a scene via menu
- **WHEN** user clicks `File → Open Scene...`
- **THEN** a file dialog opens in `./scenes`
- **AND** `SceneLoader::open_scene()` is called (clears world + full load)
- **AND** camera state is synced from the ECS entity

### Requirement: Import Scene menu

The system SHALL preserve the existing `Import Scene...` menu item with append-only semantics.

#### Scenario: Importing objects
- **WHEN** user clicks `File → Import Scene...`
- **THEN** `SceneLoader::import_scene()` is called
- **AND** only object entities are appended
- **AND** current camera and lights are preserved

### Requirement: Camera ECS sync

The system SHALL sync FlyCamera state with the ECS camera entity.

#### Scenario: Load sync
- **WHEN** a scene is opened/loaded
- **THEN** `read_camera_from_world()` initializes `self.camera` from ECS

#### Scenario: Save sync
- **WHEN** user saves a scene
- **THEN** `write_camera_to_world()` writes `self.camera` state back to ECS
- **AND** the save dialog auto-appends `.ron` if missing

### Requirement: Per-frame lighting update

The system SHALL update `LightingUniforms` from ECS each frame.

#### Scenario: Frame render
- **WHEN** each frame is rendered
- **THEN** `lighting.camera_pos` is updated from `self.camera.position`
- **AND** `lighting.light` is updated from the first `(Transform, Light)` ECS entity

## Acceptance Criteria

- [x] `Open Scene...` menu opens scenes with full replacement
- [x] `Import Scene...` appends only objects
- [x] Save dialog auto-appends `.ron`
- [x] Camera syncs from ECS on load
- [x] Camera syncs to ECS on save
- [x] Lighting updates from ECS per frame
- [x] All tests pass
- [x] `cargo clippy` 无新增警告
