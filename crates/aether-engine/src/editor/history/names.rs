use crate::ecs::components::Name;
use crate::ecs::{Entity, World};
use crate::editor::EditorError;

pub(super) fn resolve_name(world: &World, name: &str) -> Result<Entity, EditorError> {
    let matches: Vec<_> = world
        .query::<(Entity, &Name)>()
        .iter()
        .filter_map(|(entity, component)| (component.0 == name).then_some(entity))
        .collect();
    match matches.as_slice() {
        [] => Err(EditorError::EntityNotFound {
            name: name.to_string(),
        }),
        [entity] => Ok(*entity),
        _ => Err(EditorError::DuplicateName {
            name: name.to_string(),
        }),
    }
}
