# Scene Serializer Complete Implementation Specification

## Purpose

重构 `serialize_world` 使其能从 ECS 实体完整提取场景数据（相机、灯光、物体名称），实现场景 save/load 的可靠往返。

## Requirements

### Requirement: Camera extraction

The system SHALL extract camera config from the `(Transform, Camera)` ECS entity.

#### Scenario: Camera round-trip
- **WHEN** `serialize_world` traverses the world
- **THEN** it finds the first `(Transform, Camera)` entity
- **AND** extracts position, yaw, pitch, fov
- **AND** falls back to `CameraConfig::default()` if no camera entity exists

### Requirement: Light extraction

The system SHALL extract light configs from `(Transform, Light)` ECS entities.

#### Scenario: Light round-trip
- **WHEN** `serialize_world` traverses the world
- **THEN** it finds all `(Transform, Light)` entities
- **AND** extracts light_type, color, intensity
- **AND** derives direction from `Transform.rotation * Vec3::NEG_Y`

### Requirement: Object name extraction

The system SHALL read `Name` components to populate `ObjectConfig.name`.

#### Scenario: Named object save
- **WHEN** `serialize_world` traverses object entities
- **THEN** it reads the `Name` component
- **AND** writes it to `ObjectConfig.name`

### Requirement: RON round-trip

The system SHALL produce RON that can be deserialized back to an equivalent `SceneDescription`.

#### Scenario: Full round-trip
- **WHEN** a scene is loaded, modified, saved, and re-loaded
- **THEN** the `SceneDescription` before and after are equal

## Design Decisions

### D1: Direction derivation from rotation

For directional lights, the default direction is `-Y`. The `Transform.rotation` rotates this to the actual direction. So `direction = rotation * Vec3::NEG_Y`. This mirrors the logic in `loader.rs::spawn_light`.

### D2: Name is required for object entities

`serialize_world` queries `(&Transform, &MeshHandle, &MaterialUniform, &Visibility, &Name)`. Any object entity without `Name` will not be serialized. This is correct because `build_world` (from #62) always attaches `Name`.

## Acceptance Criteria

- [x] `serialize_world` extracts camera from ECS entity
- [x] `serialize_world` extracts lights from ECS entities
- [x] `serialize_world` reads `Name` for object names
- [x] RON round-trip produces equivalent `SceneDescription`
- [x] All tests pass
- [x] `cargo clippy` 无新增警告
