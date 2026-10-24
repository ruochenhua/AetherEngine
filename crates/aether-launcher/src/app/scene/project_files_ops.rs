use super::*;

fn import_dialog_directory(
    root: &std::path::Path,
    destination: &str,
    last_import_directory: Option<&std::path::PathBuf>,
) -> std::path::PathBuf {
    last_import_directory
        .filter(|path| path.is_dir())
        .cloned()
        .unwrap_or_else(|| root.join(destination.replace('/', std::path::MAIN_SEPARATOR_STR)))
}

pub(super) fn process(app: &mut App) {
    if let Some(action) = app.pending_project_file.take() {
        match action {
            crate::app::workspace::ProjectFileAction::CreateFolder { destination, name } => {
                match crate::project_files::create_folder(
                    app.workspace.project_root(),
                    &destination,
                    &name,
                ) {
                    Ok(path) => {
                        app.workspace.refresh_project();
                        app.editor_operation_message = Some(format!("Created folder {path}"));
                        info!(path, "Created project folder");
                    }
                    Err(error) => {
                        app.editor_operation_message =
                            Some(format!("Create folder failed: {error}"));
                        tracing::error!(%error, "Project folder creation failed");
                    }
                }
            }
            crate::app::workspace::ProjectFileAction::ImportAsset { destination } => {
                let root = app.workspace.project_root().to_path_buf();
                let initial = import_dialog_directory(
                    &root,
                    &destination,
                    app.last_asset_import_directory.as_ref(),
                );
                let picked = rfd::FileDialog::new()
                    .set_directory(initial)
                    .add_filter(
                        "Supported assets",
                        &["png", "jpg", "jpeg", "glb", "obj", "mtl"],
                    )
                    .pick_files();
                if let Some(sources) = picked.filter(|sources| !sources.is_empty()) {
                    app.last_asset_import_directory = sources
                        .first()
                        .and_then(|source| source.parent())
                        .map(std::path::Path::to_path_buf);
                    app.pending_asset_import = Some(crate::app::PendingAssetImport {
                        destination,
                        sources,
                        requires_confirmation: true,
                    });
                    app.pending_asset_import_confirm = false;
                }
            }
        }
    }

    let confirmed = std::mem::take(&mut app.pending_asset_import_confirm);
    if !app
        .pending_asset_import
        .as_ref()
        .is_some_and(|pending| !pending.requires_confirmation || confirmed)
    {
        return;
    }
    let Some(pending) = app.pending_asset_import.take() else {
        return;
    };
    let report = crate::project_files::import_paths(
        app.workspace.project_root(),
        &pending.destination,
        &pending.sources,
    );
    if !report.imported.is_empty() {
        app.workspace.refresh_project();
        if let Some(path) = report.imported.last() {
            let selected = app
                .workspace
                .project_root()
                .join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            app.workspace.select_resource(Some(selected));
        }
    }
    if report.failures.is_empty() {
        if let [path] = report.imported.as_slice() {
            app.editor_operation_message = Some(format!("Imported asset {path}"));
            info!(path, "Imported project asset");
        } else {
            app.editor_operation_message =
                Some(format!("Imported {} asset(s)", report.imported.len()));
            info!(count = report.imported.len(), "Imported project assets");
        }
    } else {
        app.editor_operation_message = Some(format!(
            "Imported {} asset(s); {} item(s) failed",
            report.imported.len(),
            report.failures.len()
        ));
        for failure in report.failures {
            tracing::error!(%failure, "Project asset import failed");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::import_dialog_directory;

    #[test]
    fn next_import_dialog_starts_in_the_last_external_source_directory() {
        let root = std::path::PathBuf::from("/project");
        let last = std::env::temp_dir();
        assert_eq!(
            import_dialog_directory(&root, "assets/characters", Some(&last)),
            last
        );
    }

    #[test]
    fn first_import_dialog_starts_in_the_selected_project_assets_directory() {
        let root = std::path::PathBuf::from("/project");
        assert_eq!(
            import_dialog_directory(&root, "assets/characters", None),
            root.join("assets/characters")
        );
    }
}
