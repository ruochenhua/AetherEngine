//! Opt-in GPU provenance, written by the launcher that owns the device.
use std::{fs::File, io::Write};

pub(crate) fn device_info(info: &wgpu::AdapterInfo) -> String {
    if let Some(path) = std::env::var_os("AETHER_DEVICE_METADATA_PATH") {
        let data = serde_json::json!({
            "adapter": info.name, "backend": format!("{:?}", info.backend),
            "driver": info.driver, "driver_info": info.driver_info,
            "device": info.device, "vendor": info.vendor,
            "os": std::env::consts::OS, "runner_version": "visual-case-runner/2",
        });
        let result = File::options()
            .write(true)
            .create_new(true)
            .open(path)
            .and_then(|mut file| file.write_all(data.to_string().as_bytes()));
        if let Err(error) = result {
            tracing::warn!(%error, "verification device metadata could not be written");
        }
    }
    format!("{} ({:?})", info.name, info.backend)
}
