pub(super) struct SceneInfo<'a> {
    pub gpu_info: &'a str,
    pub gpu_timer_supported: bool,
    pub scene_count: usize,
    pub fps: f32,
    pub frame_time_seconds: f32,
    pub entity_count: u32,
    pub camera: &'a aether_engine::renderer::camera::FlyCamera,
    pub input: &'a aether_engine::input::InputManager,
    pub debug_mode: i32,
    pub ssr_debug_mode: u32,
}

pub(super) fn render(ui: &mut egui::Ui, info: SceneInfo<'_>) {
    egui::CollapsingHeader::new("Scene Info")
        .default_open(false)
        .show(ui, |ui| {
            ui.label(format!("GPU: {}", info.gpu_info));
            ui.label(format!(
                "GPU Timer: {}",
                if info.gpu_timer_supported {
                    "supported"
                } else {
                    "unsupported"
                }
            ));
            ui.label(format!("Scenes: {}", info.scene_count));
            ui.separator();
            ui.label(format!("FPS: {:.1}", info.fps));
            ui.label(format!("Frame: {:.2} ms", info.frame_time_seconds * 1000.0));
            ui.label(format!("Entities: {}", info.entity_count));
            let position = info.camera.position;
            ui.label(format!(
                "Camera: ({:.1}, {:.1}, {:.1})",
                position.x, position.y, position.z
            ));
            ui.label(format!("Speed: {:.1}", info.camera.speed));
            let mode_names = [
                "Full",
                "Ambient",
                "Diffuse",
                "Specular",
                "Normals",
                "NdotL",
                "Shadow",
                "Direct",
                "IBL",
                "Alpha(P)",
                "Alpha(N)",
                "NDC(F2)",
                "EnvFix(F3)",
                "VDir(F4)",
                "SSAO(F5)",
                "CSM(F7)",
            ];
            let mode_idx = info.debug_mode.clamp(0, 15) as usize;
            ui.label(format!("Debug: [{}] {}", mode_idx, mode_names[mode_idx]));
            ui.label(format!("SSR Debug: {} (F6)", info.ssr_debug_mode));
        });
    egui::CollapsingHeader::new("Input Debug")
        .default_open(false)
        .show(ui, |ui| {
            let (mouse_dx, mouse_dy) = info.input.mouse_delta();
            ui.label(format!("Mouse delta: ({:.1}, {:.1})", mouse_dx, mouse_dy));
            ui.label(format!("Alt held: {}", info.input.alt_held()));
            ui.label(format!(
                "Left held: {}",
                info.input.mouse_held(winit::event::MouseButton::Left)
            ));
        });
}
