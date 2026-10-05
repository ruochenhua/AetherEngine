//! Local-light GPU bindings and adapter capability selection.

pub(crate) fn gpu_path_supported(limits: &wgpu::Limits) -> bool {
    limits.max_bind_groups >= 5 && limits.max_storage_buffers_per_shader_stage >= 1
}

pub(crate) fn gpu_path_enabled(requested: bool, limits: &wgpu::Limits) -> bool {
    requested && gpu_path_supported(limits)
}

pub(crate) fn required_device_limits(adapter: &wgpu::Limits) -> wgpu::Limits {
    let mut required = wgpu::Limits::default();
    required.max_bind_groups = if gpu_path_supported(adapter) {
        5
    } else {
        required.max_bind_groups.min(adapter.max_bind_groups)
    };
    required.max_storage_buffers_per_shader_stage = required
        .max_storage_buffers_per_shader_stage
        .min(adapter.max_storage_buffers_per_shader_stage);
    required
}

pub(crate) struct GpuResources {
    layout: Option<wgpu::BindGroupLayout>,
    lights_buffer: Option<wgpu::Buffer>,
    params_buffer: Option<wgpu::Buffer>,
    bind_group: Option<wgpu::BindGroup>,
}

impl GpuResources {
    pub(crate) fn new(device: &wgpu::Device, enabled: bool) -> Self {
        let layout = gpu_path_enabled(enabled, &device.limits()).then(|| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Local Lights Bind Group Layout"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Storage { read_only: true },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            })
        });

        let (lights_buffer, params_buffer, bind_group) = match layout.as_ref() {
            Some(layout) => {
                let lights_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Local Lights Storage Buffer"),
                    size: (std::mem::size_of::<crate::renderer::light::GpuLocalLight>()
                        * crate::renderer::light::MAX_LOCAL_LIGHTS)
                        as wgpu::BufferAddress,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let params_buffer = device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Local Light Params Buffer"),
                    size: std::mem::size_of::<crate::renderer::light::GpuLocalLightParams>()
                        as wgpu::BufferAddress,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Local Lights Bind Group"),
                    layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: lights_buffer.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: params_buffer.as_entire_binding(),
                        },
                    ],
                });
                (Some(lights_buffer), Some(params_buffer), Some(bind_group))
            }
            None => (None, None, None),
        };

        Self {
            layout,
            lights_buffer,
            params_buffer,
            bind_group,
        }
    }

    pub(crate) fn layout(&self) -> Option<&wgpu::BindGroupLayout> {
        self.layout.as_ref()
    }

    pub(crate) fn bind_group(&self) -> Option<&wgpu::BindGroup> {
        self.bind_group.as_ref()
    }

    pub(crate) fn write(&self, queue: &wgpu::Queue, locals: &[crate::renderer::light::LocalLight]) {
        let (Some(lights_buffer), Some(params_buffer)) = (&self.lights_buffer, &self.params_buffer)
        else {
            return;
        };

        let gpu_lights: Vec<_> = locals
            .iter()
            .copied()
            .map(crate::renderer::light::GpuLocalLight::from_local)
            .collect();
        if !gpu_lights.is_empty() {
            queue.write_buffer(lights_buffer, 0, bytemuck::cast_slice(&gpu_lights));
        }
        let params = crate::renderer::light::GpuLocalLightParams {
            count: gpu_lights.len() as u32,
            _pad: [0; 3],
        };
        queue.write_buffer(params_buffer, 0, bytemuck::bytes_of(&params));
    }
}

#[cfg(test)]
mod tests {
    use super::{gpu_path_enabled, gpu_path_supported, required_device_limits};

    #[test]
    fn local_light_gpu_path_requires_group_five_and_fragment_storage() {
        let too_few_groups = wgpu::Limits {
            max_bind_groups: 4,
            ..Default::default()
        };
        assert!(!gpu_path_supported(&too_few_groups));

        let no_fragment_storage = wgpu::Limits {
            max_bind_groups: 5,
            max_storage_buffers_per_shader_stage: 0,
            ..Default::default()
        };
        assert!(!gpu_path_supported(&no_fragment_storage));

        let supported = wgpu::Limits {
            max_bind_groups: 5,
            max_storage_buffers_per_shader_stage: 1,
            ..Default::default()
        };
        assert!(gpu_path_supported(&supported));
    }

    #[test]
    fn required_limits_request_group_five_only_when_supported() {
        let limited_adapter = wgpu::Limits {
            max_bind_groups: 4,
            max_storage_buffers_per_shader_stage: 2,
            ..Default::default()
        };
        let required = required_device_limits(&limited_adapter);
        assert_eq!(required.max_bind_groups, 4);
        assert_eq!(required.max_storage_buffers_per_shader_stage, 2);

        let capable_adapter = wgpu::Limits {
            max_bind_groups: 8,
            max_storage_buffers_per_shader_stage: 8,
            ..Default::default()
        };
        let required = required_device_limits(&capable_adapter);
        assert_eq!(required.max_bind_groups, 5);
    }

    #[test]
    fn local_light_feature_flag_can_disable_a_capable_gpu_path() {
        let capable = wgpu::Limits {
            max_bind_groups: 5,
            max_storage_buffers_per_shader_stage: 1,
            ..Default::default()
        };
        assert!(gpu_path_enabled(true, &capable));
        assert!(!gpu_path_enabled(false, &capable));

        let limited = wgpu::Limits {
            max_bind_groups: 4,
            ..Default::default()
        };
        assert!(!gpu_path_enabled(true, &limited));
    }
}
