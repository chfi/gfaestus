use crate::geometry::{Point, Rect};
use crate::reactor::Reactor;

use ash::version::DeviceV1_0;
use ash::{vk, Device};

use anyhow::Result;

use handlegraph::pathhandlegraph::PathId;
#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

use crate::app::selection::SelectionBuffer;

use crate::vulkan::{draw_system::nodes::NodeVertices, GfaestusVk};

use super::{ComputeManager, ComputePipeline};

pub struct PathViewRenderer {
    pipeline: ComputePipeline,
    descriptor_set_layout: vk::DescriptorSetLayout,

    width: usize,
    height: usize,

    path_buffer: vk::Buffer,
    path_allocation: vk_mem::Allocation,
    path_allocation_info: vk_mem::AllocationInfo,
    // path_allocation_info: Option<vk_mem::AllocationInfo>,
    output_buffer: vk::Buffer,
    output_allocation: vk_mem::Allocation,
    output_allocation_info: vk_mem::AllocationInfo,
    // path_buffer:
}

impl PathViewRenderer {
    pub fn new(app: &GfaestusVk) -> Result<Self> {
        let width = 2048;
        let height = 64;
        let size = width * height;

        let device = app.vk_context().device();

        let (path_buffer, path_allocation, path_allocation_info) = {
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_DST;
            // | vk::BufferUsageFlags::TRANSFER_SRC;
            let memory_usage = vk_mem::MemoryUsage::CpuToGpu;

            let data = vec![0u32; size];

            let (buffer, allocation, allocation_info) =
                app.create_buffer_with_data(usage, memory_usage, true, &data)?;

            app.set_debug_object_name(
                buffer,
                "Path View Renderer (Path Buffer)",
            )?;

            (buffer, allocation, allocation_info)
        };

        let (output_buffer, output_allocation, output_allocation_info) = {
            let usage = vk::BufferUsageFlags::STORAGE_BUFFER
                | vk::BufferUsageFlags::TRANSFER_SRC;

            let memory_usage = vk_mem::MemoryUsage::GpuToCpu;

            let pixels = vec![[0u8; 4]; size];

            let (buffer, allocation, allocation_info) = app
                .create_buffer_with_data(usage, memory_usage, false, &pixels)?;

            app.set_debug_object_name(
                buffer,
                "Path View Renderer (Output Buffer)",
            )?;

            (buffer, allocation, allocation_info)
        };

        let descriptor_set_layout = Self::create_descriptor_set_layout(device)?;

        let pipeline_layout = {
            // use vk::ShaderStageFlags as Flags;

            // let pc_range = vk::PushConstantRange::builder()
            //     .stage_flags(Flags::COMPUTE)
            //     .offset(0)
            //     .size(20)
            //     .build();

            // let pc_ranges = [pc_range];
            let pc_ranges = [];

            let layouts = [descriptor_set_layout];

            let layout_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&pc_ranges)
                .build();

            unsafe { device.create_pipeline_layout(&layout_info, None) }
        }?;

        let pipeline = ComputePipeline::new(
            device,
            descriptor_set_layout,
            pipeline_layout,
            crate::include_shader!("compute/path_view.comp.spv"),
        )?;

        Ok(Self {
            pipeline,
            descriptor_set_layout,

            width,
            height,

            path_buffer,
            path_allocation,
            path_allocation_info,

            output_buffer,
            output_allocation,
            output_allocation_info,
        })
    }

    pub fn load_paths(
        &mut self,
        reactor: &mut Reactor,
        paths: impl IntoIterator<Item = PathId>,
    ) -> Result<()> {

        // TODO for now hardcoded to max 64 paths
    }

    fn layout_binding() -> [vk::DescriptorSetLayoutBinding; 2] {
        use vk::ShaderStageFlags as Stages;

        //

        let path_buffer = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::COMPUTE)
            .build();

        let output_buffer = vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::COMPUTE)
            .build();

        [path_buffer, output_buffer]
    }

    fn create_descriptor_set_layout(
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout> {
        let bindings = Self::layout_binding();

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings)
            .build();

        let layout =
            unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

        Ok(layout)
    }
}
