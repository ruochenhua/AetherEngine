use super::{references_tests::Fixture, *};
use egui::{epaint::Shape, Event, Modifiers, PointerButton, Pos2, Rect, Vec2};

fn frame(panel: &mut ProjectPanel, ctx: &egui::Context, events: Vec<Event>) -> egui::FullOutput {
    ctx.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1500., 900.))),
            events,
            ..Default::default()
        },
        |ui| {
            egui::CentralPanel::default().show_inside(ui, |ui| {
                assert_eq!(
                    panel.render(ui),
                    (None, None),
                    "reference UI must not request scene opens or file operations"
                );
            });
        },
    )
}
fn text(output: &egui::FullOutput, label: &str) -> Option<Rect> {
    fn find(shape: &Shape, label: &str) -> Option<Rect> {
        match shape {
            Shape::Text(t) if t.galley.text() == label => {
                Some(Rect::from_min_size(t.pos, t.galley.size()))
            }
            Shape::Vec(shapes) => shapes.iter().find_map(|s| find(s, label)),
            _ => None,
        }
    }
    output.shapes.iter().find_map(|s| find(&s.shape, label))
}
fn click(panel: &mut ProjectPanel, ctx: &egui::Context, rect: Rect) {
    for pressed in [true, false] {
        frame(
            panel,
            ctx,
            vec![
                Event::PointerMoved(rect.center()),
                Event::PointerButton {
                    pos: rect.center(),
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                },
            ],
        );
    }
}
fn escape(panel: &mut ProjectPanel, ctx: &egui::Context) {
    frame(
        panel,
        ctx,
        vec![Event::Key {
            key: egui::Key::Escape,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::NONE,
        }],
    );
}

#[test]
fn references_details_affordance_sections_and_escape_respect_text_focus() {
    let f = Fixture::new();
    f.write("assets/m.ron", r#"(normal_texture:Some("t.png"))"#);
    let mut panel = f.panel("assets/t.png");
    let ctx = egui::Context::default();
    let output = frame(&mut panel, &ctx, vec![]);
    let show = text(&output, "Show References").expect("indexed resource needs Show References");
    click(&mut panel, &ctx, show);
    super::references_tests::finish(&mut panel);
    let output = frame(&mut panel, &ctx, vec![]);
    assert!(text(&output, "Used by").is_some());
    assert!(text(&output, "Depends on").is_some());
    assert!(text(&output, "Complete").is_some());
    assert!(text(&output, "Back to target").is_some());
    let search = text(&output, "Search resources").unwrap();
    click(&mut panel, &ctx, search);
    escape(&mut panel, &ctx);
    assert_eq!(panel.references.target(), Some("assets/t.png"));
    let output = frame(&mut panel, &ctx, vec![]);
    click(&mut panel, &ctx, text(&output, "New Folder").unwrap());
    let output = frame(&mut panel, &ctx, vec![]);
    click(&mut panel, &ctx, text(&output, "New folder").unwrap());
    escape(&mut panel, &ctx);
    assert_eq!(panel.references.target(), Some("assets/t.png"));
    let output = frame(&mut panel, &ctx, vec![]);
    click(&mut panel, &ctx, text(&output, "Back to target").unwrap());
    escape(&mut panel, &ctx);
    assert_eq!(panel.references.target(), None);
    assert_eq!(panel.selected_resource(), Some("assets/t.png"));
}

#[test]
fn references_show_action_gives_preview_focus_for_escape() {
    let f = Fixture::new();
    let mut panel = f.panel("assets/t.png");
    let ctx = egui::Context::default();
    let output = frame(&mut panel, &ctx, vec![]);
    click(&mut panel, &ctx, text(&output, "Show References").unwrap());
    assert_eq!(panel.references.target(), Some("assets/t.png"));
    escape(&mut panel, &ctx);
    assert_eq!(panel.references.target(), None);
}

fn tab_to_preview_control_and_escape(steps: usize, label: &str) {
    let f = Fixture::new();
    f.write("assets/m.ron", r#"(normal_texture:Some("t.png"))"#);
    let mut panel = f.panel("assets/t.png");
    let ctx = egui::Context::default();
    let output = frame(&mut panel, &ctx, vec![]);
    click(&mut panel, &ctx, text(&output, "Show References").unwrap());
    super::references_tests::finish(&mut panel);
    frame(&mut panel, &ctx, vec![]);
    let header = ctx.memory(|memory| memory.focused()).unwrap();
    let mut output = egui::FullOutput::default();
    for _ in 0..steps {
        for pressed in [true, false] {
            output = frame(
                &mut panel,
                &ctx,
                vec![Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
    }
    let focused = ctx.memory(|memory| memory.focused()).unwrap();
    assert_ne!(
        focused, header,
        "Tab must move from header to a preview control"
    );
    let control = ctx.read_response(focused).unwrap();
    assert!(
        control
            .rect
            .contains(text(&output, label).unwrap().center()),
        "Tab did not focus {label}"
    );
    escape(&mut panel, &ctx);
    assert_eq!(
        panel.references.target(),
        None,
        "Escape with {label} focused must close preview"
    );
    assert_eq!(panel.selected_resource(), Some("assets/t.png"));
}

#[test]
fn references_tab_focused_back_closes_on_escape() {
    tab_to_preview_control_and_escape(1, "Back to target");
}
#[test]
fn references_tab_focused_reanalyze_closes_on_escape() {
    tab_to_preview_control_and_escape(2, "Reanalyze");
}
#[test]
fn references_tab_focused_recheck_closes_on_escape() {
    tab_to_preview_control_and_escape(3, "Check for changes");
}
#[test]
fn references_tab_focused_close_closes_on_escape() {
    tab_to_preview_control_and_escape(4, "Close references");
}
#[test]
fn references_tab_focused_source_closes_on_escape() {
    tab_to_preview_control_and_escape(5, "assets/m.ron");
}
