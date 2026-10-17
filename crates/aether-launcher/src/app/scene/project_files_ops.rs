use super::*;

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
                let initial = root.join(destination.replace('/', std::path::MAIN_SEPARATOR_STR));
                let picked = rfd::FileDialog::new()
                    .set_directory(initial)
                    .add_filter("Supported assets", &["png", "jpg", "jpeg", "glb"])
                    .pick_file();
                if let Some(source) = picked {
                    app.pending_asset_import = Some(crate::app::PendingAssetImport {
                        destination,
                        source,
                    });
                    app.pending_asset_import_confirm = false;
                }
            }
        }
    }

    if !app.pending_asset_import_confirm {
        return;
    }
    app.pending_asset_import_confirm = false;
    let Some(pending) = app.pending_asset_import.take() else {
        return;
    };
    match crate::project_files::import_file(
        app.workspace.project_root(),
        &pending.destination,
        &pending.source,
    ) {
        Ok(path) => {
            app.workspace.refresh_project();
            let selected = app
                .workspace
                .project_root()
                .join(path.replace('/', std::path::MAIN_SEPARATOR_STR));
            app.workspace.select_resource(Some(selected));
            app.editor_operation_message = Some(format!("Imported asset {path}"));
            info!(path, "Imported project asset");
        }
        Err(error) => {
            app.editor_operation_message = Some(format!("Asset import failed: {error}"));
            tracing::error!(%error, source = %pending.source.display(), "Project asset import failed");
        }
    }
}
