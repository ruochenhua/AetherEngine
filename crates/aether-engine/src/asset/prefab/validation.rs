use super::{ComponentPatch, PrefabComponentKind, PrefabError, PrefabNode};
use crate::editor::ComponentRecord;
use std::collections::BTreeSet;

pub(super) fn validate_node(node: &PrefabNode, ids: &mut BTreeSet<u64>) -> Result<(), PrefabError> {
    if !ids.insert(node.instance_id) {
        return Err(PrefabError::DuplicateInstanceId(node.instance_id));
    }
    let mut kinds = BTreeSet::new();
    for component in &node.components {
        let kind = PrefabComponentKind::from(component.kind());
        if !kinds.insert(kind) {
            return Err(if kind == PrefabComponentKind::Transform {
                PrefabError::DuplicateTransform(node.instance_id)
            } else {
                PrefabError::DuplicateComponent {
                    instance_id: node.instance_id,
                    component: kind,
                }
            });
        }
        if let ComponentRecord::Transform {
            translation,
            rotation_xyzw,
            scale,
        } = component
        {
            if !translation.iter().all(|value| value.is_finite())
                || !scale.iter().all(|value| value.is_finite())
                || !rotation_xyzw.iter().all(|value| value.is_finite())
                || rotation_xyzw.iter().map(|value| value * value).sum::<f32>() <= f32::EPSILON
            {
                return Err(PrefabError::InvalidTransform(node.instance_id));
            }
        }
    }
    if !kinds.contains(&PrefabComponentKind::Transform) {
        return Err(PrefabError::MissingTransform(node.instance_id));
    }
    for child in &node.children {
        validate_node(child, ids)?;
    }
    Ok(())
}

pub(super) fn collect_node_ids(root: &PrefabNode) -> BTreeSet<u64> {
    fn visit(node: &PrefabNode, output: &mut BTreeSet<u64>) {
        output.insert(node.instance_id);
        for child in &node.children {
            visit(child, output);
        }
    }
    let mut output = BTreeSet::new();
    visit(root, &mut output);
    output
}

pub(super) fn find_node(root: &PrefabNode, instance_id: u64) -> Option<&PrefabNode> {
    if root.instance_id == instance_id {
        return Some(root);
    }
    root.children
        .iter()
        .find_map(|child| find_node(child, instance_id))
}

pub(super) fn find_node_mut(root: &mut PrefabNode, instance_id: u64) -> Option<&mut PrefabNode> {
    if root.instance_id == instance_id {
        return Some(root);
    }
    root.children
        .iter_mut()
        .find_map(|child| find_node_mut(child, instance_id))
}

pub(super) fn apply_patch(
    node: &mut PrefabNode,
    patch: &ComponentPatch,
) -> Result<(), PrefabError> {
    let instance_id = node.instance_id;
    let invalid = || PrefabError::InvalidPatch {
        instance_id,
        component: patch.component(),
    };
    let record = node
        .components
        .iter_mut()
        .find(|record| PrefabComponentKind::from(record.kind()) == patch.component());
    match patch {
        ComponentPatch::TransformTranslation { value, .. } => match record {
            Some(ComponentRecord::Transform { translation, .. })
                if value.iter().all(|item| item.is_finite()) =>
            {
                *translation = *value
            }
            _ => return Err(invalid()),
        },
        ComponentPatch::TransformRotation { value, .. } => match record {
            Some(ComponentRecord::Transform { rotation_xyzw, .. })
                if value.iter().all(|item| item.is_finite())
                    && value.iter().map(|item| item * item).sum::<f32>() > f32::EPSILON =>
            {
                *rotation_xyzw = *value
            }
            _ => return Err(invalid()),
        },
        ComponentPatch::TransformScale { value, .. } => match record {
            Some(ComponentRecord::Transform { scale, .. })
                if value.iter().all(|item| item.is_finite()) =>
            {
                *scale = *value
            }
            _ => return Err(invalid()),
        },
        ComponentPatch::MeshSource { source, .. } => match record {
            Some(ComponentRecord::Mesh { source: target }) => *target = source.clone(),
            _ => return Err(invalid()),
        },
        ComponentPatch::MaterialConfig { config, .. } => match record {
            Some(ComponentRecord::Material { config: target }) => *target = config.clone(),
            _ => return Err(invalid()),
        },
        ComponentPatch::Visibility { visible, .. } => match record {
            Some(ComponentRecord::Visibility { visible: target }) => *target = *visible,
            _ => return Err(invalid()),
        },
        ComponentPatch::NameValue { value, .. } => match record {
            Some(ComponentRecord::Name { value: target }) => *target = value.clone(),
            None => node.name = value.clone(),
            _ => return Err(invalid()),
        },
        ComponentPatch::LightConfig { config, .. } => match record {
            Some(ComponentRecord::Light { config: target }) => *target = config.clone(),
            _ => return Err(invalid()),
        },
        ComponentPatch::CameraConfig { config, .. } => match record {
            Some(ComponentRecord::Camera { config: target }) => *target = config.clone(),
            _ => return Err(invalid()),
        },
        ComponentPatch::AtmosphereConfig { config, .. } => match record {
            Some(ComponentRecord::Atmosphere { config: target }) => *target = config.clone(),
            _ => return Err(invalid()),
        },
        ComponentPatch::CloudsConfig { config, .. } => match record {
            Some(ComponentRecord::Clouds { config: target }) => *target = config.clone(),
            _ => return Err(invalid()),
        },
    }
    Ok(())
}
