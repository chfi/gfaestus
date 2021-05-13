use ash::{
    extensions::{
        ext::DebugReport,
        khr::{Surface, Swapchain},
    },
    version::{DeviceV1_0, EntryV1_0, InstanceV1_0},
    vk::SurfaceKHR,
};
use ash::{vk, Device, Entry};

use std::{ffi::CString, ops::RangeInclusive};

use anyhow::Result;

pub struct NodeTranslatePipeline {
    pub(super) descriptor_pool: vk::DescriptorPool,
    pub(super) descriptor_set_layout: vk::DescriptorSetLayout,

    pub(super) vertices_set: vk::DescriptorSet,

    pub(super) pipeline_layout: vk::PipelineLayout,
    pub(super) pipeline: vk::Pipeline,

    pub(super) device: Device,
}

impl NodeTranslatePipeline {
    pub fn new(device: &Device) -> Result<Self> {
        let desc_set_layout = Self::create_descriptor_set_layout(device)?;

        let (pipeline, pipeline_layout) =
            Self::create_pipeline(device, desc_set_layout);

        let descriptor_pool = {
            let pool_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: 1,
            };

            let pool_sizes = [pool_size];

            let pool_info = vk::DescriptorPoolCreateInfo::builder()
                .pool_sizes(&pool_sizes)
                .max_sets(1)
                .build();

            unsafe { device.create_descriptor_pool(&pool_info, None) }
        }?;

        let descriptor_sets = {
            let layouts = vec![desc_set_layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(descriptor_pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout: desc_set_layout,

            vertices_set: descriptor_sets[0],

            pipeline_layout,
            pipeline,

            device: device.clone(),
        })
    }

    fn create_descriptor_set_layout(
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout> {
        let binding = Self::layout_binding();
        let bindings = [binding];

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings)
            .build();

        let layout =
            unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

        Ok(layout)
    }

    fn layout_binding() -> vk::DescriptorSetLayoutBinding {
        use vk::ShaderStageFlags as Stages;

        vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::COMPUTE)
            .build()
    }

    fn create_pipeline(
        device: &Device,
        descriptor_set_layout: vk::DescriptorSetLayout,
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        let comp_src =
            crate::load_shader!("../../shaders/node_translate.comp.spv");

        let comp_module =
            super::draw_system::create_shader_module(device, &comp_src);

        let entry_point = CString::new("main").unwrap();

        let comp_state_info = vk::PipelineShaderStageCreateInfo::builder()
            .stage(vk::ShaderStageFlags::COMPUTE)
            .module(comp_module)
            .name(&entry_point)
            .build();

        let shader_state_infos = [comp_state_info];

        let layout = {
            use vk::ShaderStageFlags as Flags;

            /*
            let pc_range = vk::PushConstantRange::builder()
                .stage_flags(Flags::COMPUTE)
                .offset(0)
                .size(8)
                .build();
            */

            // let pc_ranges = [pc_range];
            let pc_ranges = [];

            let layouts = [descriptor_set_layout];

            let layout_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&pc_ranges)
                .build();

            unsafe {
                device.create_pipeline_layout(&layout_info, None).unwrap()
            }
        };

        let pipeline_info = vk::ComputePipelineCreateInfo::builder()
            .layout(layout)
            .stage(comp_state_info)
            .build();

        let pipeline_infos = [pipeline_info];

        let pipeline = unsafe {
            device
                .create_compute_pipelines(
                    vk::PipelineCache::null(),
                    &pipeline_infos,
                    None,
                )
                .unwrap()[0]
        };

        unsafe {
            device.destroy_shader_module(comp_module, None);
        }

        (pipeline, layout)
    }
}
