//! Surface and offscreen frame targets used by the launcher.

use aether_engine::renderer::context::RenderContext;
use winit::event_loop::ActiveEventLoop;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FrameTargetKind {
    Surface,
    Offscreen,
}

fn frame_target_kind(should_screenshot: bool) -> FrameTargetKind {
    if should_screenshot {
        FrameTargetKind::Offscreen
    } else {
        FrameTargetKind::Surface
    }
}

pub(crate) enum FrameTarget {
    Surface(wgpu::SurfaceTexture),
    Offscreen(wgpu::Texture),
}

impl FrameTarget {
    /// Acquire a presented or screenshot-only render target.
    pub(crate) fn acquire(
        ctx: &mut RenderContext,
        event_loop: &ActiveEventLoop,
        should_screenshot: bool,
    ) -> Option<Self> {
        if frame_target_kind(should_screenshot) == FrameTargetKind::Offscreen {
            let view_formats = if ctx.render_target_format() != ctx.surface_format() {
                vec![ctx.render_target_format()]
            } else {
                vec![]
            };
            return Some(Self::Offscreen(ctx.device.create_texture(
                &wgpu::TextureDescriptor {
                    label: Some("Screenshot Render Target"),
                    size: wgpu::Extent3d {
                        width: ctx.config.width,
                        height: ctx.config.height,
                        depth_or_array_layers: 1,
                    },
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: wgpu::TextureDimension::D2,
                    format: ctx.surface_format(),
                    usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
                    view_formats: &view_formats,
                },
            )));
        }

        match ctx.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(output)
            | wgpu::CurrentSurfaceTexture::Suboptimal(output) => Some(Self::Surface(output)),
            wgpu::CurrentSurfaceTexture::Lost => {
                ctx.resize(ctx.config.width, ctx.config.height);
                None
            }
            wgpu::CurrentSurfaceTexture::Validation => {
                event_loop.exit();
                None
            }
            wgpu::CurrentSurfaceTexture::Timeout
            | wgpu::CurrentSurfaceTexture::Occluded
            | wgpu::CurrentSurfaceTexture::Outdated => None,
        }
    }

    pub(crate) fn texture(&self) -> &wgpu::Texture {
        match self {
            Self::Surface(output) => &output.texture,
            Self::Offscreen(texture) => texture,
        }
    }

    pub(crate) fn present(self) {
        if let Self::Surface(output) = self {
            output.present();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screenshot_frames_bypass_surface_acquisition() {
        assert_eq!(frame_target_kind(true), FrameTargetKind::Offscreen);
        assert_eq!(frame_target_kind(false), FrameTargetKind::Surface);
    }
}
