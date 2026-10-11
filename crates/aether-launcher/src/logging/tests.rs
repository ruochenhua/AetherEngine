use super::*;
use tracing_subscriber::prelude::*;

#[test]
fn ew1_capture_preserves_real_structured_event() {
    let (layer, receiver) = capture(2);
    tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), || {
        tracing::error!(target: "scene", path = "scenes/13_clouds.ron", attempts = 3u64, "cannot load scene");
    });
    let event = receiver
        .try_recv()
        .expect("real tracing event must be captured");
    assert!(event.timestamp_ms > 0);
    assert_eq!(event.level, "ERROR");
    assert_eq!(event.target, "scene");
    assert_eq!(event.message, "cannot load scene");
    assert_eq!(event.fields["path"], "scenes/13_clouds.ron");
    assert_eq!(event.fields["attempts"], 3);
}

#[test]
fn ew1_concurrent_producers_bound_queue_and_count_overflow() {
    let (layer, receiver) = capture(4);
    let health = layer.health.clone();
    let dispatch = tracing::Dispatch::new(tracing_subscriber::registry().with(layer));
    std::thread::scope(|scope| {
        for _ in 0..4 {
            let dispatch = dispatch.clone();
            scope.spawn(move || {
                tracing::dispatcher::with_default(&dispatch, || {
                    for _ in 0..100 {
                        tracing::error!("overflow");
                    }
                })
            });
        }
    });
    assert_eq!(receiver.try_iter().count(), 4);
    assert_eq!(health.dropped(), [396, 0, 0, 0, 0]);
    assert!(health.degraded());
}

fn event(level: &str, target: &str, message: &str) -> CapturedEvent {
    CapturedEvent {
        timestamp_ms: 1,
        level: level.into(),
        target: target.into(),
        message: message.into(),
        fields: Default::default(),
        resource: None,
    }
}
#[test]
fn ew1_view_filters_clear_and_pause_never_stop_collection() {
    let mut store = LogStore::new(3);
    let mut view = LogView::default();
    store.push(event("INFO", "asset", "loaded"));
    store.push(event("ERROR", "scene", "missing cloud"));
    view.filter.level = 0;
    view.filter.category = "scene".into();
    view.filter.text = "CLOUD".into();
    assert_eq!(store.visible(&view.filter).count(), 1);
    assert_eq!(store.events.len(), 2);
    view.paused = true;
    assert!(!view.auto_scroll());
    store.push(event("ERROR", "scene", "cloud rejected"));
    assert_eq!(store.visible(&view.filter).count(), 2);
    view.paused = false;
    assert!(view.auto_scroll());
    store.clear();
    assert!(store.events.is_empty());
    for _ in 0..5 {
        store.push(event("ERROR", "scene", "cloud after clear"));
    }
    assert_eq!(store.events.len(), 3);
    assert_eq!(store.visible(&view.filter).count(), 3);
}

fn directory(label: &str) -> std::path::PathBuf {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = std::env::temp_dir().join(format!(
        "aether-ew1-{label}-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&path).unwrap();
    path
}
fn file_config(
    directory: std::path::PathBuf,
    max_bytes: u64,
    retained_files: usize,
) -> file::FileConfig {
    file::FileConfig {
        directory,
        max_bytes,
        retained_files,
    }
}
#[test]
fn ew1_file_sink_writes_admitted_records_and_counts_overflow() {
    let directory = directory("file");
    let (sink, receiver) = file::file_channel(2);
    for i in 0..5 {
        sink.send(event("WARN", "asset", &format!("{i}")));
    }
    assert_eq!(sink.health.dropped(), [0, 3, 0, 0, 0]);
    assert!(sink.health.degraded());
    let mut writer = file::FileWriter::start(
        receiver,
        sink.health.clone(),
        file_config(directory.clone(), 4096, 3),
    );
    writer.shutdown();
    let records = std::fs::read_to_string(directory.join("runtime.0.jsonl")).unwrap_or_default();
    assert_eq!(records.lines().count(), 2);
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn ew1_file_failure_is_stable_and_falls_back_once() {
    let directory = directory("failure");
    let blocked = directory.join("blocked");
    std::fs::write(&blocked, "not a directory").unwrap();
    let (sink, receiver) = file::file_channel(8);
    for _ in 0..5 {
        sink.send(event("ERROR", "asset", "unwritable"));
    }
    let summaries = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let captured = summaries.clone();
    let mut writer = file::FileWriter::start_with_fallback(
        receiver,
        sink.health.clone(),
        file_config(blocked, 4096, 3),
        move |summary| captured.lock().unwrap().push(summary.to_owned()),
    );
    writer.shutdown();
    assert!(sink.health.degraded());
    let error = sink.health.error().expect("stable writer error");
    writer.shutdown();
    assert_eq!(sink.health.error().as_deref(), Some(error.as_str()));
    assert_eq!(
        sink.health
            .fallback_count
            .load(std::sync::atomic::Ordering::Relaxed),
        1
    );
    let summaries = summaries.lock().unwrap();
    assert_eq!(
        summaries.len(),
        1,
        "fallback emitter must receive exactly one summary"
    );
    assert_eq!(summaries[0], format!("Runtime logging degraded: {error}"));
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn ew1_shutdown_flushes_and_rotation_bounds_files_across_sessions() {
    let directory = directory("rotation");
    for _ in 0..2 {
        let (sink, receiver) = file::file_channel(8);
        for i in 0..8 {
            sink.send(event("INFO", "test", &format!("admitted-{i}")));
        }
        let mut writer = file::FileWriter::start(
            receiver,
            sink.health.clone(),
            file_config(directory.clone(), 256, 3),
        );
        writer.shutdown();
        let paths: Vec<_> = std::fs::read_dir(&directory)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(paths.len(), 3);
        for path in paths {
            assert!(std::fs::metadata(&path).unwrap().len() <= 256);
        }
        let current = std::fs::read_to_string(directory.join("runtime.0.jsonl")).unwrap();
        assert!(current.contains("admitted-7"));
        assert!(current.ends_with('\n'));
    }
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn ew1_no_gui_overlay_still_collects_and_persists() {
    let directory = directory("no-gui");
    let (layer, receiver) = capture(8);
    let health = layer.health.clone();
    let (sink, file_receiver) = file::file_channel(8);
    let mut writer = file::FileWriter::start(
        file_receiver,
        sink.health.clone(),
        file_config(directory.clone(), 4096, 3),
    );
    let mut app = crate::app::App::new(
        crate::app::cli::parse_args(&["launcher".into(), "--no-gui-overlay".into()]).unwrap(),
    );
    app.state = crate::app::LauncherState::Running {
        world: aether_engine::ecs::World::new(),
        lighting: Default::default(),
    };
    app.logs = LogPanel::new(
        Some(receiver),
        health,
        sink.health.clone(),
        directory.clone(),
    );
    tracing::subscriber::with_default(
        tracing_subscriber::registry().with(layer.with_file(sink)),
        || tracing::info!("headless diagnostic"),
    );
    crate::app::ui::draw(&mut app, 0.0, Default::default());
    assert_eq!(app.logs.store.events.len(), 1);
    writer.shutdown();
    let records = std::fs::read_to_string(directory.join("runtime.0.jsonl")).unwrap();
    assert!(records.contains("headless diagnostic"));
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn ew1_resource_links_require_structured_fields_and_stay_inside_project() {
    let directory = directory("links");
    std::fs::create_dir_all(directory.join("assets")).unwrap();
    std::fs::write(directory.join("assets/cloud.ron"), "cloud").unwrap();
    let panel = LogPanel::new(
        None,
        Default::default(),
        Default::default(),
        directory.clone(),
    );
    let mut event = event("ERROR", "asset", "assets/cloud.ron");
    assert!(panel.resource(&event).is_none());
    event
        .fields
        .insert("asset".into(), "assets/cloud.ron".into());
    assert_eq!(panel.resource(&event), Some("assets/cloud.ron".into()));
    event
        .fields
        .insert("asset".into(), r"assets\cloud.ron".into());
    assert_eq!(panel.resource(&event), Some("assets/cloud.ron".into()));
    for path in [
        "../escape.ron",
        "/tmp/escape.ron",
        "assets/../../escape.ron",
        "C:\\escape.ron",
        "assets/missing.ron",
        r"..\escape.ron",
        r"assets\..\..\escape.ron",
        r"\server\escape.ron",
    ] {
        event.fields.insert("asset".into(), path.into());
        assert!(panel.resource(&event).is_none(), "{path}");
    }
    std::fs::remove_dir_all(directory).unwrap();
}
#[test]
fn ew1_real_scene_error_is_visible_in_menu_log_panel() {
    let (layer, receiver) = capture(8);
    let health = layer.health.clone();
    let mut app = crate::app::App::new(crate::app::cli::parse_args(&["launcher".into()]).unwrap());
    app.workspace
        .select_tab(crate::app::workspace::WorkspaceTab::Log);
    app.logs = LogPanel::new(
        Some(receiver),
        health,
        Default::default(),
        std::env::current_dir().unwrap(),
    );
    let path = std::path::Path::new("scenes/ew1_missing_scene.ron");
    let error = aether_engine::scene::loader::SceneLoader::from_file(path).unwrap_err();
    tracing::subscriber::with_default(tracing_subscriber::registry().with(layer), || {
        crate::app::scene::report_load_error(path, &error)
    });
    let output = crate::app::ui::draw(
        &mut app,
        0.0,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 720.0),
            )),
            ..Default::default()
        },
    );
    fn contains(shape: &egui::epaint::Shape) -> bool {
        match shape {
            egui::epaint::Shape::Text(text) => text.galley.text().contains("Open scene error"),
            egui::epaint::Shape::Vec(shapes) => shapes.iter().any(contains),
            _ => false,
        }
    }
    assert!(output.shapes.iter().any(|shape| contains(&shape.shape)));
}

fn menu_frame(app: &mut crate::app::App, events: Vec<egui::Event>) -> egui::FullOutput {
    crate::app::ui::draw(
        app,
        0.0,
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1280.0, 720.0),
            )),
            events,
            ..Default::default()
        },
    )
}
fn label_position(output: &egui::FullOutput, label: &str) -> egui::Pos2 {
    fn find(shape: &egui::epaint::Shape, label: &str) -> Option<egui::Pos2> {
        match shape {
            egui::epaint::Shape::Text(text) if text.galley.text().contains(label) => {
                Some(text.pos + text.galley.size() * 0.5)
            }
            egui::epaint::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, label)),
            _ => None,
        }
    }
    output
        .shapes
        .iter()
        .find_map(|shape| find(&shape.shape, label))
        .expect(label)
}
fn menu_click(app: &mut crate::app::App, position: egui::Pos2) -> egui::FullOutput {
    menu_frame(
        app,
        vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: Default::default(),
            },
        ],
    );
    menu_frame(
        app,
        vec![egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: Default::default(),
        }],
    )
}
#[test]
fn ew1_resource_link_click_selects_project_resource() {
    let directory = directory("project-selection");
    std::fs::write(directory.join("cloud.ron"), "cloud").unwrap();
    let mut app = crate::app::App::new(crate::app::cli::parse_args(&["launcher".into()]).unwrap());
    app.logs = LogPanel::new(
        None,
        Default::default(),
        Default::default(),
        directory.clone(),
    );
    let mut record = event("ERROR", "asset", "resource diagnostic");
    record.fields.insert("asset".into(), "cloud.ron".into());
    app.logs.store.push(record);
    app.workspace
        .select_tab(crate::app::workspace::WorkspaceTab::Log);
    menu_frame(&mut app, vec![]);
    let output = menu_frame(&mut app, vec![]);
    let row = label_position(&output, "resource diagnostic");
    menu_click(&mut app, row);
    let output = menu_frame(&mut app, vec![]);
    let link = label_position(&output, "Show in Project: cloud.ron");
    menu_click(&mut app, link);
    assert_eq!(app.workspace.selected_resource, Some("cloud.ron".into()));
    assert_eq!(
        app.workspace.tab,
        crate::app::workspace::WorkspaceTab::Project
    );
    std::fs::remove_dir_all(directory).unwrap();
}
