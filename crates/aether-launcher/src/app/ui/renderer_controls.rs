//! Collapsible controls for render features.

use aether_engine::renderer::passes::{fxaa::FxaaQuality, tone_mapping::ToneMappingMode};

pub(super) struct RenderOptions<'a> {
    pub(super) ssao_enabled: &'a mut bool,
    pub(super) ssao_radius: &'a mut f32,
    pub(super) ssao_bias: &'a mut f32,
    pub(super) ssao_intensity: &'a mut f32,
    pub(super) shadow_enabled: &'a mut bool,
    pub(super) ibl_enabled: &'a mut bool,
    pub(super) ssr_enabled: &'a mut bool,
    pub(super) tone_mapping_mode: &'a mut ToneMappingMode,
    pub(super) bloom_enabled: &'a mut bool,
    pub(super) bloom_threshold: &'a mut f32,
    pub(super) bloom_intensity: &'a mut f32,
    pub(super) fxaa_enabled: &'a mut bool,
    pub(super) fxaa_quality: &'a mut FxaaQuality,
    pub(super) fxaa_edge_threshold: &'a mut Option<f32>,
}

pub(super) fn render(ui: &mut egui::Ui, options: &mut RenderOptions<'_>) {
    ui.checkbox(options.ssao_enabled, "SSAO");
    if *options.ssao_enabled {
        ui.add(
            egui::Slider::new(options.ssao_radius, 0.01..=2.0)
                .logarithmic(true)
                .text("SSAO Radius (world)"),
        );
        ui.add(
            egui::Slider::new(options.ssao_bias, 0.001..=0.2)
                .logarithmic(true)
                .text("SSAO Bias (world)"),
        );
        ui.add(egui::Slider::new(options.ssao_intensity, 0.0..=4.0).text("SSAO Intensity"));
    }
    ui.checkbox(options.shadow_enabled, "Shadow Map");
    ui.checkbox(options.ibl_enabled, "IBL");
    ui.checkbox(options.ssr_enabled, "SSR");
    egui::ComboBox::from_label("Tone Mapping")
        .selected_text(format!("{:?}", *options.tone_mapping_mode))
        .show_ui(ui, |ui| {
            ui.selectable_value(options.tone_mapping_mode, ToneMappingMode::Off, "Off");
            ui.selectable_value(
                options.tone_mapping_mode,
                ToneMappingMode::Reinhard,
                "Reinhard",
            );
            ui.selectable_value(options.tone_mapping_mode, ToneMappingMode::ACES, "ACES");
        });
    ui.checkbox(options.bloom_enabled, "Bloom");
    if *options.bloom_enabled {
        ui.add(egui::Slider::new(options.bloom_threshold, 0.0..=3.0).text("Bloom Threshold"));
        ui.add(egui::Slider::new(options.bloom_intensity, 0.0..=2.0).text("Bloom Intensity"));
    }
    ui.checkbox(options.fxaa_enabled, "FXAA");
    if *options.fxaa_enabled {
        egui::ComboBox::from_label("FXAA Quality")
            .selected_text(format!("{:?}", *options.fxaa_quality))
            .show_ui(ui, |ui| {
                ui.selectable_value(options.fxaa_quality, FxaaQuality::Low, "Low");
                ui.selectable_value(options.fxaa_quality, FxaaQuality::Medium, "Medium");
                ui.selectable_value(options.fxaa_quality, FxaaQuality::High, "High");
            });
        let mut custom = options.fxaa_edge_threshold.is_some();
        ui.checkbox(&mut custom, "Custom Edge Threshold");
        if custom {
            let threshold =
                options
                    .fxaa_edge_threshold
                    .get_or_insert_with(|| match *options.fxaa_quality {
                        FxaaQuality::Low => 0.063,
                        FxaaQuality::Medium => 0.031,
                        FxaaQuality::High => 0.016,
                    });
            ui.add(
                egui::Slider::new(threshold, 0.001..=0.1)
                    .logarithmic(true)
                    .text("Edge Threshold"),
            );
        } else {
            *options.fxaa_edge_threshold = None;
        }
    }
}
