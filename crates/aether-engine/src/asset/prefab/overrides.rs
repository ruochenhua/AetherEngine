use super::validation::{apply_patch, collect_node_ids, find_node, find_node_mut};
use super::{PrefabComponentKind, PrefabDocument, PrefabError, PrefabOverrides};
use std::collections::BTreeSet;

impl PrefabOverrides {
    /// Apply typed patches and removals to a copy of a document atomically.
    pub fn apply_to(&self, document: &mut PrefabDocument) -> Result<(), PrefabError> {
        document.validate()?;
        let mut next = document.clone();
        let node_ids = collect_node_ids(&next.root);
        let mut patched = BTreeSet::new();
        let mut patched_components = BTreeSet::new();
        for patch in &self.patches {
            let (instance_id, field) = patch.target();
            let component = patch.component();
            if !patched.insert((instance_id, field)) {
                return Err(PrefabError::DuplicatePatch {
                    instance_id,
                    field: field.name(),
                });
            }
            patched_components.insert((instance_id, component));
            if !node_ids.contains(&instance_id) {
                return Err(PrefabError::InvalidPatch {
                    instance_id,
                    component,
                });
            }
            let node = find_node(&next.root, instance_id).ok_or(PrefabError::InvalidPatch {
                instance_id,
                component,
            })?;
            if component != PrefabComponentKind::Name
                && !node
                    .components
                    .iter()
                    .any(|record| PrefabComponentKind::from(record.kind()) == component)
            {
                return Err(PrefabError::InvalidPatch {
                    instance_id,
                    component,
                });
            }
            apply_patch(
                find_node_mut(&mut next.root, instance_id).ok_or(PrefabError::InvalidPatch {
                    instance_id,
                    component,
                })?,
                patch,
            )?;
        }

        let mut removed = BTreeSet::new();
        for removal in &self.removed_components {
            let key = (removal.instance_id, removal.component);
            if !removed.insert(key) {
                return Err(PrefabError::DuplicateComponent {
                    instance_id: removal.instance_id,
                    component: removal.component,
                });
            }
            if patched_components.contains(&(removal.instance_id, removal.component)) {
                return Err(PrefabError::PatchAndRemoveConflict {
                    instance_id: removal.instance_id,
                    component: removal.component,
                });
            }
            if removal.component == PrefabComponentKind::Transform {
                return Err(PrefabError::RemoveTransform(removal.instance_id));
            }
            let node = find_node_mut(&mut next.root, removal.instance_id).ok_or(
                PrefabError::InvalidPatch {
                    instance_id: removal.instance_id,
                    component: removal.component,
                },
            )?;
            let was_present = node
                .components
                .iter()
                .any(|record| PrefabComponentKind::from(record.kind()) == removal.component)
                || removal.component == PrefabComponentKind::Name;
            if !was_present {
                return Err(PrefabError::InvalidPatch {
                    instance_id: removal.instance_id,
                    component: removal.component,
                });
            }
            node.components
                .retain(|record| PrefabComponentKind::from(record.kind()) != removal.component);
        }
        next.validate()?;
        *document = next;
        Ok(())
    }
}
