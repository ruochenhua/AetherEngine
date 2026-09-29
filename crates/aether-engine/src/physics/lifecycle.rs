//! Entity-to-Rapier lifecycle and transform synchronization.

use super::components::{Collider, ColliderList, PhysicsDesc, RigidBody, TransformAuthority};
use super::runtime::{EntityPhysicsState, PhysicsRuntime};
use super::types::PhysicsError;
use super::validation::{collider_builder, validate_desc, validate_samples, validate_transform};
use crate::ecs::{components::Transform, World};
use glam::{Quat, Vec3};
use rapier3d::na::{Isometry3, Quaternion, Translation3, UnitQuaternion};
use rapier3d::prelude::RigidBodyBuilder;
use std::collections::BTreeMap;

impl PhysicsRuntime {
    /// Creates a body with up to four attached colliders. Validation and the
    /// duplicate check happen before allocating any Rapier handles.
    pub fn spawn_entity(
        &mut self,
        entity_bits: u64,
        transform: &Transform,
        desc: &PhysicsDesc,
    ) -> Result<(), PhysicsError> {
        if self.entity_state.contains_key(&entity_bits) {
            return Err(PhysicsError::DuplicateEntity { entity_bits });
        }
        validate_desc(entity_bits, transform, desc)?;

        let mut collider_builders = desc
            .colliders
            .iter()
            .map(|collider| collider_builder(entity_bits, transform.scale, collider))
            .collect::<Result<Vec<_>, _>>()?;
        if !desc.body.is_static {
            let unit_density_mass = collider_builders
                .iter()
                .map(|builder| builder.build().mass_properties().mass())
                .sum::<f32>();
            if !unit_density_mass.is_finite() || unit_density_mass <= 0.0 {
                return Err(PhysicsError::InvalidRigidBody {
                    entity_bits,
                    reason: "collider mass properties must be finite and positive",
                });
            }
            let density = desc.body.mass / unit_density_mass;
            for builder in &mut collider_builders {
                *builder = builder.clone().density(density);
            }
        } else {
            for builder in &mut collider_builders {
                *builder = builder.clone().density(0.0);
            }
        }
        let pose = transform_pose(transform);
        let body_builder = if desc.body.is_static {
            RigidBodyBuilder::fixed()
        } else {
            RigidBodyBuilder::dynamic()
        };
        let body = body_builder
            .position(pose)
            .linvel(super::validation::vector3(desc.body.velocity))
            .angvel(super::validation::vector3(desc.body.angular_velocity))
            .build();
        let body_handle = self.bodies.insert(body);
        self.allocation_stats.body_handles_created += 1;
        let collider_handles = collider_builders
            .into_iter()
            .map(|builder| {
                let handle = self.colliders.insert_with_parent(
                    builder.build(),
                    body_handle,
                    &mut self.bodies,
                );
                self.allocation_stats.collider_handles_created += 1;
                handle
            })
            .collect::<Vec<_>>();

        self.entity_to_body.insert(entity_bits, body_handle);
        self.entity_to_colliders
            .insert(entity_bits, collider_handles.clone());
        self.body_to_entity.insert(body_handle, entity_bits);
        for handle in &collider_handles {
            self.collider_to_entity.insert(*handle, entity_bits);
        }
        self.entity_state.insert(
            entity_bits,
            EntityPhysicsState {
                desc: desc.clone(),
                scale: transform.scale,
                initial_transform: transform.clone(),
                body_handle,
                collider_handles,
            },
        );
        self.query_pipeline.update(&self.colliders);
        Ok(())
    }

    /// Removes one body and all colliders attached to it.
    pub fn remove_entity(&mut self, entity_bits: u64) -> Result<(), PhysicsError> {
        self.remove_entity_checked(entity_bits)
    }

    /// Reconciles physics components with the ECS world and applies scene-owned
    /// poses. All candidate descriptions are validated before changing state.
    pub(super) fn sync_in(&mut self, world: &World) -> Result<(), PhysicsError> {
        let mut desired = BTreeMap::<u64, (Transform, PhysicsDesc)>::new();
        for (entity, transform, body, collider, authority) in world
            .query::<(
                hecs::Entity,
                &Transform,
                &RigidBody,
                &Collider,
                Option<&TransformAuthority>,
            )>()
            .iter()
        {
            let desc = PhysicsDesc {
                body: body.clone(),
                colliders: vec![collider.clone()],
                authority: authority.copied().unwrap_or_else(|| authority_for(body)),
            };
            insert_desired(&mut desired, entity.to_bits().get(), transform, desc)?;
        }
        for (entity, transform, body, colliders, authority) in world
            .query::<(
                hecs::Entity,
                &Transform,
                &RigidBody,
                &ColliderList,
                Option<&TransformAuthority>,
            )>()
            .iter()
        {
            let desc = PhysicsDesc {
                body: body.clone(),
                colliders: colliders.0.clone(),
                authority: authority.copied().unwrap_or_else(|| authority_for(body)),
            };
            insert_desired(&mut desired, entity.to_bits().get(), transform, desc)?;
        }

        for (entity_bits, (transform, desc)) in &desired {
            validate_desc(*entity_bits, transform, desc)?;
        }
        self.validate_all_handles()?;

        let removed = self
            .entity_state
            .keys()
            .filter(|entity_bits| !desired.contains_key(entity_bits))
            .copied()
            .collect::<Vec<_>>();
        for entity_bits in removed {
            self.remove_entity_checked(entity_bits)?;
        }

        for (entity_bits, (transform, desc)) in desired {
            let needs_rebuild = self
                .entity_state
                .get(&entity_bits)
                .is_some_and(|state| state.desc != desc || state.scale != transform.scale);
            if needs_rebuild {
                self.remove_entity_checked(entity_bits)?;
            }
            if !self.entity_state.contains_key(&entity_bits) {
                self.spawn_entity(entity_bits, &transform, &desc)?;
                continue;
            }

            let state = self
                .entity_state
                .get(&entity_bits)
                .cloned()
                .ok_or(PhysicsError::StaleHandle { entity_bits })?;
            if state.desc.authority == TransformAuthority::Scene || self.paused {
                let body = self
                    .bodies
                    .get_mut(state.body_handle)
                    .ok_or(PhysicsError::StaleHandle { entity_bits })?;
                body.set_position(transform_pose(&transform), true);
                if let Some(state) = self.entity_state.get_mut(&entity_bits) {
                    state.initial_transform = transform.clone();
                }
            }
        }
        self.query_pipeline.update(&self.colliders);
        Ok(())
    }

    /// Copies dynamic body poses back to their matching ECS transforms.
    pub(super) fn sync_out(&self, world: &mut World) -> Result<(), PhysicsError> {
        let poses = self
            .entity_state
            .iter()
            .filter(|(_, state)| state.desc.authority == TransformAuthority::Physics)
            .map(|(entity_bits, state)| {
                let body = self
                    .bodies
                    .get(state.body_handle)
                    .ok_or(PhysicsError::StaleHandle {
                        entity_bits: *entity_bits,
                    })?;
                let p = body.translation();
                let q = body.rotation().quaternion();
                Ok((
                    *entity_bits,
                    Vec3::new(p.x, p.y, p.z),
                    Quat::from_xyzw(q.i, q.j, q.k, q.w),
                ))
            })
            .collect::<Result<Vec<_>, PhysicsError>>()?;

        for (entity, transform) in world
            .query_mut::<(hecs::Entity, &mut Transform)>()
            .into_iter()
        {
            if let Some((_, translation, rotation)) = poses
                .iter()
                .find(|(entity_bits, _, _)| *entity_bits == entity.to_bits().get())
            {
                transform.translation = *translation;
                transform.rotation = *rotation;
            }
        }
        Ok(())
    }

    /// Clears all scene-owned Rapier state and restarts the deterministic step
    /// sequence. Safe to call repeatedly, including during scene replacement.
    pub fn reset(&mut self) -> Result<(), PhysicsError> {
        self.clear_state();
        self.last_step_index = 0;
        self.fixed_dt = None;
        self.paused = false;
        self.last_seek_frame = None;
        Ok(())
    }

    /// Releases all handles and leaves this runtime ready for a future scene.
    pub fn shutdown(&mut self) -> Result<(), PhysicsError> {
        self.reset()
    }

    /// Restarts Rapier state when deterministic playback publishes a new seek
    /// target, restoring scene-authored transforms before replay.
    pub fn prepare_seek_frame(
        &mut self,
        world: &mut World,
        frame_time: &crate::time::FrameTime,
    ) -> Result<(), PhysicsError> {
        validate_samples(&frame_time.samples, 0, None)?;
        if self.last_seek_frame.as_ref() == Some(frame_time) {
            return Ok(());
        }
        if self.last_step_index > 0 {
            let initial = self
                .entity_state
                .iter()
                .map(|(entity_bits, state)| (*entity_bits, state.initial_transform.clone()))
                .collect::<BTreeMap<_, _>>();
            for (entity, transform) in world
                .query_mut::<(hecs::Entity, &mut Transform)>()
                .into_iter()
            {
                if let Some(initial_transform) = initial.get(&entity.to_bits().get()) {
                    *transform = initial_transform.clone();
                }
            }
            self.clear_state();
            self.last_step_index = 0;
            self.fixed_dt = None;
        }
        self.last_seek_frame = Some(frame_time.clone());
        Ok(())
    }

    /// Pauses stepping while still allowing scene-authored transforms to sync in.
    pub fn set_paused(&mut self, paused: bool) {
        self.paused = paused;
    }

    /// Returns whether stepping is currently paused.
    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Number of entities with registered rigid bodies.
    pub fn entity_count(&self) -> usize {
        self.entity_state.len()
    }

    /// Number of registered colliders.
    pub fn collider_count(&self) -> usize {
        self.collider_to_entity.len()
    }

    /// Number of colliders registered for an entity, if it has a body.
    pub fn entity_collider_count(&self, entity_bits: u64) -> Option<usize> {
        self.entity_to_colliders.get(&entity_bits).map(Vec::len)
    }

    fn validate_all_handles(&self) -> Result<(), PhysicsError> {
        for (entity_bits, state) in &self.entity_state {
            if self.entity_to_body.get(entity_bits) != Some(&state.body_handle)
                || self.body_to_entity.get(&state.body_handle) != Some(entity_bits)
                || self.bodies.get(state.body_handle).is_none()
                || self.entity_to_colliders.get(entity_bits) != Some(&state.collider_handles)
                || state.collider_handles.iter().any(|handle| {
                    self.colliders.get(*handle).is_none()
                        || self.collider_to_entity.get(handle) != Some(entity_bits)
                })
            {
                return Err(PhysicsError::StaleHandle {
                    entity_bits: *entity_bits,
                });
            }
        }
        Ok(())
    }

    fn remove_entity_checked(&mut self, entity_bits: u64) -> Result<(), PhysicsError> {
        let state = self
            .entity_state
            .get(&entity_bits)
            .cloned()
            .ok_or(PhysicsError::StaleHandle { entity_bits })?;
        self.validate_entity_handles(entity_bits, &state)?;
        self.bodies.remove(
            state.body_handle,
            &mut self.islands,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            true,
        );
        self.entity_state.remove(&entity_bits);
        self.entity_to_body.remove(&entity_bits);
        self.entity_to_colliders.remove(&entity_bits);
        self.body_to_entity.remove(&state.body_handle);
        for handle in state.collider_handles {
            self.collider_to_entity.remove(&handle);
        }
        self.query_pipeline.update(&self.colliders);
        Ok(())
    }

    fn validate_entity_handles(
        &self,
        entity_bits: u64,
        state: &EntityPhysicsState,
    ) -> Result<(), PhysicsError> {
        if self.entity_to_body.get(&entity_bits) != Some(&state.body_handle)
            || self.body_to_entity.get(&state.body_handle) != Some(&entity_bits)
            || self.bodies.get(state.body_handle).is_none()
            || self.entity_to_colliders.get(&entity_bits) != Some(&state.collider_handles)
            || state.collider_handles.iter().any(|handle| {
                self.colliders.get(*handle).is_none()
                    || self.collider_to_entity.get(handle) != Some(&entity_bits)
            })
        {
            return Err(PhysicsError::StaleHandle { entity_bits });
        }
        Ok(())
    }

    fn clear_state(&mut self) {
        self.pipeline = rapier3d::prelude::PhysicsPipeline::new();
        self.islands = rapier3d::prelude::IslandManager::new();
        self.broad_phase = rapier3d::prelude::BroadPhaseMultiSap::new();
        self.narrow_phase = rapier3d::prelude::NarrowPhase::new();
        self.bodies = rapier3d::prelude::RigidBodySet::new();
        self.colliders = rapier3d::prelude::ColliderSet::new();
        self.impulse_joints = rapier3d::prelude::ImpulseJointSet::new();
        self.multibody_joints = rapier3d::prelude::MultibodyJointSet::new();
        self.ccd_solver = rapier3d::prelude::CCDSolver::new();
        self.query_pipeline = rapier3d::prelude::QueryPipeline::new();
        self.entity_to_body.clear();
        self.entity_to_colliders.clear();
        self.body_to_entity.clear();
        self.collider_to_entity.clear();
        self.entity_state.clear();
    }
}

fn insert_desired(
    desired: &mut BTreeMap<u64, (Transform, PhysicsDesc)>,
    entity_bits: u64,
    transform: &Transform,
    desc: PhysicsDesc,
) -> Result<(), PhysicsError> {
    validate_transform(entity_bits, transform)?;
    if desired
        .insert(entity_bits, (transform.clone(), desc))
        .is_some()
    {
        return Err(PhysicsError::DuplicateEntity { entity_bits });
    }
    Ok(())
}

fn authority_for(body: &RigidBody) -> TransformAuthority {
    if body.is_static {
        TransformAuthority::Scene
    } else {
        TransformAuthority::Physics
    }
}

fn transform_pose(transform: &Transform) -> Isometry3<f32> {
    let rotation = transform.rotation.normalize();
    Isometry3::from_parts(
        Translation3::new(
            transform.translation.x,
            transform.translation.y,
            transform.translation.z,
        ),
        UnitQuaternion::from_quaternion(Quaternion::new(
            rotation.w, rotation.x, rotation.y, rotation.z,
        )),
    )
}
