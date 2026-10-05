//! Stable scene hierarchy labels and operation targets.

use aether_engine::ecs::components::{MeshHandle, Name};
use aether_engine::ecs::{Entity, World};

pub(super) struct HierarchyItem {
    pub(super) entity: Entity,
    pub(super) name: String,
    pub(super) operation_name: Option<String>,
}

pub(super) fn collect(world: &World) -> Vec<HierarchyItem> {
    let mut raw = Vec::new();
    for (entity, mesh) in world.query::<(Entity, &MeshHandle)>().iter() {
        let operation_name = world
            .query_one::<&Name>(entity)
            .get()
            .ok()
            .map(|name| name.0.clone());
        raw.push((entity, mesh.name.clone(), operation_name));
    }
    for (entity, name) in world.query::<(Entity, &Name)>().iter() {
        if world.query_one::<&MeshHandle>(entity).get().is_err() {
            raw.push((entity, name.0.clone(), Some(name.0.clone())));
        }
    }
    raw.sort_by(|a, b| a.1.cmp(&b.1));

    let mut operation_name_counts = std::collections::HashMap::new();
    for (_, _, name) in &raw {
        if let Some(name) = name {
            *operation_name_counts.entry(name.clone()).or_insert(0usize) += 1;
        }
    }
    let mut name_counts = std::collections::HashMap::new();
    raw.into_iter()
        .map(|(entity, base_name, operation_name)| {
            let count = name_counts.entry(base_name.clone()).or_insert(0usize);
            let display_name = if *count == 0 {
                base_name
            } else {
                format!("{} ({})", base_name, count)
            };
            *count += 1;
            HierarchyItem {
                entity,
                name: display_name,
                operation_name: operation_name
                    .filter(|name| operation_name_counts.get(name) == Some(&1)),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_actor_names_are_displayed_but_not_valid_operation_targets() {
        let mut world = World::new();
        world.spawn((Name("Duplicate".into()),));
        world.spawn((Name("Unique".into()),));
        world.spawn((Name("Duplicate".into()),));

        let items = collect(&world);
        assert_eq!(
            items
                .iter()
                .map(|item| item.name.as_str())
                .collect::<Vec<_>>(),
            ["Duplicate", "Duplicate (1)", "Unique"]
        );
        assert!(items[..2].iter().all(|item| item.operation_name.is_none()));
        assert_eq!(items[2].operation_name.as_deref(), Some("Unique"));
    }
}
