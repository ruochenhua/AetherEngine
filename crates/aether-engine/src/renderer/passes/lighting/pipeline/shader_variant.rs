use super::shaders::LIGHTING_SHADER_SRC;
use std::borrow::Cow;

/// Build the direction-only shader variant for adapters that cannot bind the
/// fifth group or a fragment storage buffer.
pub(super) fn shader_source(local_lights_supported: bool) -> Cow<'static, str> {
    if local_lights_supported {
        return Cow::Borrowed(LIGHTING_SHADER_SRC);
    }

    let Some(source) = replace_marked_section(
        LIGHTING_SHADER_SRC,
        "// LOCAL_LIGHT_BINDINGS_BEGIN",
        "// LOCAL_LIGHT_BINDINGS_END",
        "",
    ) else {
        return Cow::Borrowed(LIGHTING_SHADER_SRC);
    };
    let Some(source) = replace_marked_section(
        &source,
        "// LOCAL_LIGHT_EVALUATION_BEGIN",
        "// LOCAL_LIGHT_EVALUATION_END",
        "    let local_direct = vec3<f32>(0.0);\n    let direct_with_locals = direct_light;",
    ) else {
        return Cow::Borrowed(LIGHTING_SHADER_SRC);
    };
    Cow::Owned(source)
}

fn replace_marked_section(
    source: &str,
    start_marker: &str,
    end_marker: &str,
    replacement: &str,
) -> Option<String> {
    let start = source.find(start_marker)?;
    let end_marker_start = source[start..]
        .find(end_marker)
        .map(|offset| start + offset)?;
    let mut end = end_marker_start + end_marker.len();
    if source[end..].starts_with("\r\n") {
        end += 2;
    } else if source[end..].starts_with('\n') {
        end += 1;
    }
    Some(format!(
        "{}{}{}",
        &source[..start],
        replacement,
        &source[end..]
    ))
}
