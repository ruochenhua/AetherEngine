//! Strict material document shape validation at drop/replay time.
use aether_engine::scene::MaterialConfig;

pub(super) fn decode(source: &str) -> anyhow::Result<MaterialConfig> {
    let ron::value::Value::Map(fields) = ron::from_str(source)? else {
        anyhow::bail!("Material Asset must contain a material configuration document");
    };
    const FIELDS: &[&str] = &[
        "albedo",
        "roughness",
        "metallic",
        "unlit",
        "albedo_texture",
        "normal_texture",
        "orm_texture",
        "emissive_texture",
        "normal_scale",
        "occlusion_strength",
        "emissive",
        "emissive_intensity",
        "orm_swizzle",
        "transparent",
    ];
    anyhow::ensure!(!fields.is_empty() && fields.keys().all(|key| matches!(key, ron::value::Value::String(name) if FIELDS.contains(&name.as_str()))), "resource is not a recognized Material configuration document");
    Ok(ron::from_str(source)?)
}
