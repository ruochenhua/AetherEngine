use super::{cli::parse_args, scene, ui, workspace::ProjectFileAction, App};
use egui::{epaint::Shape, Event, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use std::path::PathBuf;

fn app() -> App {
    App::new(parse_args(&["aether-launcher".into()]).unwrap())
}

fn frame(app: &mut App, events: Vec<Event>) -> egui::FullOutput {
    ui::draw(
        app,
        1.0 / 60.0,
        RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1280.0, 720.0))),
            events,
            ..Default::default()
        },
    )
}

fn text_rect(output: &egui::FullOutput, label: &str) -> Option<Rect> {
    fn find(shape: &Shape, label: &str) -> Option<Rect> {
        match shape {
            Shape::Text(text) if text.galley.text() == label => {
                Some(Rect::from_min_size(text.pos, text.galley.size()))
            }
            Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape, label))
}

fn text_starts_with(output: &egui::FullOutput, prefix: &str) -> bool {
    fn find(shape: &Shape, prefix: &str) -> bool {
        match shape {
            Shape::Text(text) => text.galley.text().starts_with(prefix),
            Shape::Vec(shapes) => shapes.iter().any(|shape| find(shape, prefix)),
            _ => false,
        }
    }
    output.shapes.iter().any(|shape| find(&shape.shape, prefix))
}

fn click(app: &mut App, position: Pos2) -> egui::FullOutput {
    frame(
        app,
        vec![
            Event::PointerMoved(position),
            Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: true,
                modifiers: Modifiers::default(),
            },
        ],
    );
    frame(
        app,
        vec![
            Event::PointerMoved(position),
            Event::PointerButton {
                pos: position,
                button: PointerButton::Primary,
                pressed: false,
                modifiers: Modifiers::default(),
            },
        ],
    )
}

fn valid_project() -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "aether_project_file_ui_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(root.join("scenes")).unwrap();
    std::fs::create_dir_all(root.join("assets")).unwrap();
    std::fs::write(
        root.join("scenes/test.ron"),
        "(name: \"Test\", camera: (position: (0.0, 0.0, 0.0)))",
    )
    .unwrap();
    root
}

#[test]
fn ew4_project_actions_show_the_captured_destination_directory() {
    let mut app = app();
    let root = valid_project();
    app.workspace.set_project_root(root.clone());

    let output = frame(&mut app, vec![]);
    click(&mut app, text_rect(&output, "Assets").unwrap().center());
    let output = frame(&mut app, vec![]);

    assert!(text_rect(&output, "Destination: assets").is_some());
    assert!(text_rect(&output, "New Folder").is_some());
    assert!(text_rect(&output, "Import Assets…").is_some());
    click(
        &mut app,
        text_rect(&output, "Import Assets…").unwrap().center(),
    );
    assert_eq!(
        app.pending_project_file,
        Some(ProjectFileAction::ImportAsset {
            destination: "assets".into(),
        })
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn external_asset_drop_is_accepted_only_over_the_assets_resource_list() {
    let mut app = app();
    let root = valid_project();
    app.workspace.set_project_root(root.clone());

    let output = frame(&mut app, vec![]);
    click(&mut app, text_rect(&output, "Assets").unwrap().center());
    frame(&mut app, vec![]);

    let target = app
        .workspace
        .asset_drop_target_rect()
        .expect("Project resource list should expose its drop target");
    assert!(app.workspace.accepts_asset_drop_at(target.center()));
    assert!(!app
        .workspace
        .accepts_asset_drop_at(egui::Pos2::new(1.0, 1.0)));

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ew4_new_folder_form_emits_a_deferred_action_for_the_current_directory() {
    let mut app = app();
    let root = valid_project();
    app.workspace.set_project_root(root.clone());
    let output = frame(&mut app, vec![]);
    click(&mut app, text_rect(&output, "Assets").unwrap().center());
    let output = frame(&mut app, vec![]);
    click(&mut app, text_rect(&output, "New Folder").unwrap().center());
    let output = frame(&mut app, vec![]);
    let field = text_rect(&output, "New folder").expect("folder name field");
    click(&mut app, field.center());
    frame(&mut app, vec![Event::Text("ImportedModels".into())]);
    let output = frame(&mut app, vec![]);
    click(&mut app, text_rect(&output, "Create").unwrap().center());

    assert_eq!(
        app.pending_project_file,
        Some(ProjectFileAction::CreateFolder {
            destination: "assets".into(),
            name: "ImportedModels".into(),
        })
    );
    assert!(!root.join("assets/ImportedModels").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ew4_create_folder_action_mutates_disk_after_the_ui_frame() {
    let mut app = app();
    let root = valid_project();
    app.workspace.set_project_root(root.clone());
    app.pending_project_file = Some(ProjectFileAction::CreateFolder {
        destination: "assets".into(),
        name: "new-content".into(),
    });

    scene::process_post_ui_ops(&mut app);

    assert!(root.join("assets/new-content").is_dir());
    assert_eq!(
        app.editor_operation_message.as_deref(),
        Some("Created folder assets/new-content")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ew4_import_confirmation_commits_one_validated_file_after_confirmation() {
    let mut app = app();
    let root = valid_project();
    let source = root.join("outside-source.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([40, 90, 160, 255]))
        .save(&source)
        .unwrap();
    app.workspace.set_project_root(root.clone());
    app.pending_asset_import = Some(super::PendingAssetImport {
        destination: "assets".into(),
        sources: vec![source.clone()],
        requires_confirmation: true,
    });

    let output = frame(&mut app, vec![]);
    assert!(text_starts_with(&output, "Target: assets"));
    click(&mut app, text_rect(&output, "Import").unwrap().center());
    assert!(app.pending_asset_import_confirm);
    assert!(!root.join("assets/outside-source.png").exists());

    scene::process_post_ui_ops(&mut app);

    assert!(root.join("assets/outside-source.png").is_file());
    assert!(app.pending_asset_import.is_none());
    assert_eq!(
        app.editor_operation_message.as_deref(),
        Some("Imported asset assets/outside-source.png")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ew4_import_confirmation_commits_all_selected_files_as_one_batch() {
    let mut app = app();
    let root = valid_project();
    let sources = ["first.png", "second.png"]
        .into_iter()
        .map(|name| {
            let path = root.join(name);
            image::RgbaImage::from_pixel(2, 2, image::Rgba([40, 90, 160, 255]))
                .save(&path)
                .unwrap();
            path
        })
        .collect::<Vec<_>>();
    app.workspace.set_project_root(root.clone());
    app.pending_asset_import = Some(super::PendingAssetImport {
        destination: "assets".into(),
        sources: sources.clone(),
        requires_confirmation: true,
    });

    let output = frame(&mut app, vec![]);
    assert!(text_rect(&output, "Files: 2 selected").is_some());
    click(&mut app, text_rect(&output, "Import").unwrap().center());
    scene::process_post_ui_ops(&mut app);

    assert!(root.join("assets/first.png").is_file());
    assert!(root.join("assets/second.png").is_file());
    assert_eq!(
        app.editor_operation_message.as_deref(),
        Some("Imported 2 asset(s)")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ew4_external_drop_imports_without_a_confirmation_step() {
    let mut app = app();
    let root = valid_project();
    let source = root.join("dropped.png");
    image::RgbaImage::from_pixel(2, 2, image::Rgba([40, 90, 160, 255]))
        .save(&source)
        .unwrap();
    app.workspace.set_project_root(root.clone());
    let output = frame(&mut app, vec![]);
    click(&mut app, text_rect(&output, "Assets").unwrap().center());
    frame(&mut app, vec![]);
    let target = app.workspace.asset_drop_target_rect().unwrap();
    assert!(super::handler::queue_asset_drop(
        &mut app,
        source,
        target.center()
    ));

    scene::process_post_ui_ops(&mut app);

    assert!(root.join("assets/dropped.png").is_file());
    assert!(app.pending_asset_import.is_none());
    assert_eq!(
        app.editor_operation_message.as_deref(),
        Some("Imported asset assets/dropped.png")
    );
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn ew4_external_folder_drop_imports_nested_assets_without_confirmation() {
    let mut app = app();
    let root = valid_project();
    let source_root = std::env::temp_dir().join(format!(
        "aether_folder_drop_{}_{}",
        std::process::id(),
        root.file_name().unwrap().to_string_lossy()
    ));
    let source_folder = source_root.join("ImportedPack");
    std::fs::create_dir_all(source_folder.join("Textures")).unwrap();
    image::RgbaImage::from_pixel(2, 2, image::Rgba([40, 90, 160, 255]))
        .save(source_folder.join("Textures/Checker.png"))
        .unwrap();
    std::fs::write(
        source_folder.join("mesh.obj"),
        "mtllib mesh.mtl\nv 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n",
    )
    .unwrap();
    std::fs::write(
        source_folder.join("mesh.mtl"),
        "newmtl default\nmap_Kd Textures/Checker.png\n",
    )
    .unwrap();
    app.workspace.set_project_root(root.clone());

    let output = frame(&mut app, vec![]);
    click(&mut app, text_rect(&output, "Assets").unwrap().center());
    frame(&mut app, vec![]);
    let target = app.workspace.asset_drop_target_rect().unwrap();
    assert!(super::handler::queue_asset_drop(
        &mut app,
        source_folder,
        target.center()
    ));

    scene::process_post_ui_ops(&mut app);

    assert!(root
        .join("assets/ImportedPack/Textures/Checker.png")
        .is_file());
    assert!(root.join("assets/ImportedPack/mesh.obj").is_file());
    assert!(root.join("assets/ImportedPack/mesh.mtl").is_file());
    assert!(app.pending_asset_import.is_none());
    assert_eq!(
        app.editor_operation_message.as_deref(),
        Some("Imported 3 asset(s)")
    );
    std::fs::remove_dir_all(root).unwrap();
    std::fs::remove_dir_all(source_root).unwrap();
}

#[test]
fn ew4_empty_created_folders_remain_visible_in_the_project_tree() {
    let mut app = app();
    let root = valid_project();
    std::fs::create_dir(root.join("assets/empty-folder")).unwrap();
    app.workspace.set_project_root(root.clone());

    let output = frame(&mut app, vec![]);
    click(&mut app, text_rect(&output, "Assets").unwrap().center());
    let output = frame(&mut app, vec![]);
    assert!(text_rect(&output, "empty-folder").is_some());
    std::fs::remove_dir_all(root).unwrap();
}
