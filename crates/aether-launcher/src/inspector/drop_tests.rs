use super::{render, InspectorTarget};
use crate::{
    app::workspace::{AssetDragPayload, MaterialDropRequest, MaterialDropTarget},
    project_index::ResourceKind,
};
use aether_engine::{
    ecs::{components::Transform, World},
    scene::{material::TextureUsage, MaterialConfig},
};
use egui::{epaint::Shape, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};

fn text_pos(output: &egui::FullOutput, label: &str) -> Pos2 {
    fn find(shape: &Shape, label: &str) -> Option<Pos2> {
        match shape {
            Shape::Text(t) if t.galley.text() == label => Some(t.pos + t.galley.size() * 0.5),
            Shape::Vec(v) => v.iter().find_map(|s| find(s, label)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|s| find(&s.shape, label))
        .unwrap_or_else(|| panic!("missing {label}"))
}
fn frame(
    ctx: &egui::Context,
    target: &mut InspectorTarget,
    kind: ResourceKind,
    events: Vec<Event>,
) -> (egui::FullOutput, Option<MaterialDropRequest>) {
    let mut request = None;
    let output = ctx.run_ui(
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(900.0, 1200.0))),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                ui.label("Resource")
                    .interact(egui::Sense::drag())
                    .dnd_set_drag_payload(AssetDragPayload {
                        kind,
                        relative_path: "assets/resource".into(),
                    });
                request = render(ui, target);
            });
        },
    );
    (output, request)
}
fn drag(
    kind: ResourceKind,
    label: &str,
    linked: bool,
    cancel: bool,
    outside: bool,
) -> Option<MaterialDropRequest> {
    let ctx = egui::Context::default();
    let mut world = World::new();
    let entity = world.spawn(());
    let initial = MaterialConfig::default();
    let mut target = InspectorTarget::Mesh {
        entity,
        transform: Transform::default(),
        material: initial.clone(),
        material_asset: linked.then(|| "assets/material.ron".into()),
        material_asset_error: None,
        detach_material: false,
        euler: [0.0; 3],
    };
    let (output, _) = frame(&ctx, &mut target, kind, vec![]);
    let source = text_pos(&output, "Resource");
    let destination = if outside {
        Pos2::new(880.0, 1150.0)
    } else {
        text_pos(&output, label)
    };
    frame(
        &ctx,
        &mut target,
        kind,
        vec![
            Event::PointerMoved(source),
            Event::PointerButton {
                pos: source,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
        ],
    );
    frame(
        &ctx,
        &mut target,
        kind,
        vec![Event::PointerMoved(source + Vec2::new(15.0, 0.0))],
    );
    let (_hover, _) = frame(
        &ctx,
        &mut target,
        kind,
        vec![Event::PointerMoved(destination)],
    );
    let (hover, _) = frame(&ctx, &mut target, kind, vec![]);
    if !cancel && !outside {
        let message = if linked && label != "Material Asset" {
            "Linked material: use Detach and edit first"
        } else if label == "Material Asset" && kind != ResourceKind::Material {
            "Material Asset accepts Material resources only"
        } else if label != "Material Asset" && kind != ResourceKind::Image {
            "Texture slots accept Image resources only"
        } else {
            "Release to assign resource"
        };
        text_pos(&hover, message);
    }
    if cancel {
        frame(
            &ctx,
            &mut target,
            kind,
            vec![Event::Key {
                key: egui::Key::Escape,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::default(),
            }],
        );
    }
    let (_, request) = frame(
        &ctx,
        &mut target,
        kind,
        vec![Event::PointerButton {
            pos: destination,
            button: PointerButton::Primary,
            pressed: false,
            modifiers: Modifiers::default(),
        }],
    );
    if let InspectorTarget::Mesh {
        material,
        detach_material,
        ..
    } = target
    {
        assert_eq!(material, initial, "UI capture cannot assign material");
        assert!(!detach_material, "a drop cannot detach");
    }
    request
}
#[test]
fn real_egui_material_drop_links_or_relinks_only_at_asset_row() {
    for linked in [false, true] {
        assert_eq!(
            drag(
                ResourceKind::Material,
                "Material Asset",
                linked,
                false,
                false
            )
            .unwrap()
            .target,
            MaterialDropTarget::Asset
        );
    }
}
#[test]
fn real_egui_image_drop_captures_each_inline_texture_target() {
    for (label, slot) in [
        ("Albedo Texture", TextureUsage::Albedo),
        ("Normal Texture", TextureUsage::Normal),
        ("ORM Texture", TextureUsage::Orm),
        ("Emissive Texture", TextureUsage::Emissive),
    ] {
        assert_eq!(
            drag(ResourceKind::Image, label, false, false, false)
                .unwrap()
                .target,
            MaterialDropTarget::Texture(slot)
        );
    }
}
#[test]
fn real_egui_wrong_type_linked_texture_cancel_and_outside_drops_are_noops() {
    for (kind, label, linked, cancel, outside) in [
        (ResourceKind::Obj, "Material Asset", false, false, false),
        (ResourceKind::Image, "Material Asset", false, false, false),
        (
            ResourceKind::Material,
            "Albedo Texture",
            false,
            false,
            false,
        ),
        (ResourceKind::Image, "Albedo Texture", true, false, false),
        (ResourceKind::Image, "Albedo Texture", false, true, false),
        (ResourceKind::Image, "Albedo Texture", false, false, true),
    ] {
        assert!(drag(kind, label, linked, cancel, outside).is_none());
    }
}
