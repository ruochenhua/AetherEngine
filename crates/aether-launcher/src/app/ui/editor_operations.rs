//! UI controls that emit replayable editor operations.

use aether_engine::editor::{ComponentRecord, EditorOperation};
use aether_engine::renderer::light::LightType;
use aether_engine::scene::LightConfig;

pub(super) fn add_light_button(
    ui: &mut egui::Ui,
    selected_name: Option<&str>,
    has_saved_scene: bool,
) -> (egui::Response, Option<EditorOperation>) {
    add_typed_light_button(
        ui,
        selected_name,
        has_saved_scene,
        LightType::Point,
        "Add Point Light to Selected",
    )
}

pub(super) fn add_spot_light_button(
    ui: &mut egui::Ui,
    selected_name: Option<&str>,
    has_saved_scene: bool,
) -> (egui::Response, Option<EditorOperation>) {
    add_typed_light_button(
        ui,
        selected_name,
        has_saved_scene,
        LightType::Spot,
        "Add Spot Light to Selected",
    )
}

fn add_typed_light_button(
    ui: &mut egui::Ui,
    selected_name: Option<&str>,
    has_saved_scene: bool,
    light_type: LightType,
    label: &str,
) -> (egui::Response, Option<EditorOperation>) {
    let enabled = selected_name.is_some() && has_saved_scene;
    let response = ui.add_enabled(enabled, egui::Button::new(label));
    let operation = if response.clicked() {
        selected_name.map(|entity_name| {
            ui_event_to_operation(AddLightEvent {
                entity_name: entity_name.to_owned(),
                light_type,
            })
        })
    } else {
        None
    };
    (response, operation)
}

struct AddLightEvent {
    entity_name: String,
    light_type: LightType,
}

fn ui_event_to_operation(event: AddLightEvent) -> EditorOperation {
    EditorOperation::AddComponent {
        entity_name: event.entity_name,
        component: ComponentRecord::Light {
            config: LightConfig {
                light_type: event.light_type,
                ..LightConfig::default()
            },
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
    use serde_json::json;
    use std::fs;

    fn draw(
        context: &egui::Context,
        events: Vec<Event>,
        light_type: LightType,
    ) -> (Option<EditorOperation>, Rect) {
        let input = RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(640.0, 480.0))),
            events,
            ..RawInput::default()
        };
        let mut operation = None;
        let mut button_rect = Rect::NOTHING;
        let _ = context.run_ui(input, |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                let (response, emitted) = if light_type == LightType::Spot {
                    add_spot_light_button(ui, Some("Selected"), true)
                } else {
                    add_light_button(ui, Some("Selected"), true)
                };
                button_rect = response.rect;
                operation = emitted;
            });
        });
        (operation, button_rect)
    }

    #[test]
    fn add_light_button_emits_operation_for_selected_entity() {
        let context = egui::Context::default();
        let (_, button_rect) = draw(&context, Vec::new(), LightType::Point);
        let position = button_rect.center();
        let _ = draw(
            &context,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
            LightType::Point,
        );
        let (operation, _) = draw(
            &context,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::default(),
                },
            ],
            LightType::Point,
        );
        let expected = EditorOperation::AddComponent {
            entity_name: "Selected".into(),
            component: ComponentRecord::Light {
                config: LightConfig {
                    light_type: LightType::Point,
                    ..LightConfig::default()
                },
            },
        };
        assert_eq!(operation.as_ref(), Some(&expected));
        let encoded = serde_json::to_value(operation.unwrap()).unwrap();
        assert_eq!(encoded["op"], "add_component");
        assert_eq!(encoded["entity_name"], "Selected");
        assert_eq!(
            encoded["component"]["Light"]["config"]["light_type"],
            "Point"
        );

        if let Some(path) = std::env::var_os("AETHER_T1_EDITOR_UI_EVENTS") {
            fs::write(
                path,
                serde_json::to_vec_pretty(&json!({
                    "case_id": "t1_editor_command_replay",
                    "kind": "Fixture",
                    "probes": [
                        { "name": "ui_event_to_operation", "value": { "kind": "Bool", "value": true } }
                    ]
                }))
                .unwrap(),
            )
            .unwrap();
        }
    }

    #[test]
    fn spot_light_button_emits_spot_component_for_selected_entity() {
        let context = egui::Context::default();
        let (_, button_rect) = draw(&context, Vec::new(), LightType::Spot);
        let position = button_rect.center();
        let _ = draw(
            &context,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed: true,
                    modifiers: Modifiers::default(),
                },
            ],
            LightType::Spot,
        );
        let (operation, _) = draw(
            &context,
            vec![
                Event::PointerMoved(position),
                Event::PointerButton {
                    pos: position,
                    button: PointerButton::Primary,
                    pressed: false,
                    modifiers: Modifiers::default(),
                },
            ],
            LightType::Spot,
        );
        assert_eq!(
            operation,
            Some(EditorOperation::AddComponent {
                entity_name: "Selected".into(),
                component: ComponentRecord::Light {
                    config: LightConfig {
                        light_type: LightType::Spot,
                        ..LightConfig::default()
                    },
                },
            })
        );
    }

    #[test]
    fn add_light_button_is_disabled_without_selection_or_saved_scene() {
        let context = egui::Context::default();
        let input = RawInput::default();
        let mut enabled_without_scene = true;
        let _ = context.run_ui(input, |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                let (response, operation) = add_light_button(ui, Some("Selected"), false);
                enabled_without_scene = response.enabled() || operation.is_some();
            });
        });
        assert!(!enabled_without_scene);
    }
}
