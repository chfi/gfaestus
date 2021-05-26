use crate::geometry::{Point, Rect};

use ash::version::DeviceV1_0;
use ash::{vk, Device};

use anyhow::Result;

use crate::app::node_flags::SelectionBuffer;

use crate::vulkan::{draw_system::nodes::NodeVertices, GfaestusVk};

use super::{ComputeManager, ComputePipeline};

pub struct Binner {
    step_1: ComputePipeline,
    step_2: ComputePipeline,
    step_3: ComputePipeline,

    buffers: BinBuffers,
}

impl Binner {
    pub fn new(app: &GfaestusVk, node_count: usize) -> Result<Self> {
        let bin_count = 32 * 32; // roughly close choice for now

        let buffers = BinBuffers::new(app, node_count, bin_count)?;

        let device = app.vk_context().device();

        let step_1 = {
            let (desc_set_layout, pipeline_layout) = Self::layouts(device)?;

            ComputePipeline::new(
                device,
                desc_set_layout,
                pipeline_layout,
                crate::include_shader!("compute/bin1.comp.spv"),
            )
        };

        let step_2 = {
            let (desc_set_layout, pipeline_layout) = Self::layouts(device)?;

            ComputePipeline::new(
                device,
                desc_set_layout,
                pipeline_layout,
                crate::include_shader!("compute/bin2.comp.spv"),
            )
        };

        let step_3 = {
            let (desc_set_layout, pipeline_layout) = Self::layouts(device)?;

            ComputePipeline::new(
                device,
                desc_set_layout,
                pipeline_layout,
                crate::include_shader!("compute/bin3.comp.spv"),
            )
        };

        Ok(Self {
            buffers,
            step_1,
            step_2,
            step_3,
        })
    }

    fn layouts(
        device: &Device,
    ) -> Result<(vk::DescriptorSetLayout, vk::PipelineLayout)> {
        let desc_set_layout = Self::create_descriptor_set_layout(device)?;

        let pipeline_layout = {
            use vk::ShaderStageFlags as Flags;

            let pc_range = vk::PushConstantRange::builder()
                .stage_flags(Flags::COMPUTE)
                .offset(0)
                .size(24)
                .build();

            let pc_ranges = [pc_range];

            let layouts = [desc_set_layout];

            let layout_info = vk::PipelineLayoutCreateInfo::builder()
                .set_layouts(&layouts)
                .push_constant_ranges(&pc_ranges)
                .build();

            unsafe { device.create_pipeline_layout(&layout_info, None) }
        }?;

        Ok((desc_set_layout, pipeline_layout))
    }

    fn layout_binding() -> [vk::DescriptorSetLayoutBinding; 5] {
        use vk::ShaderStageFlags as Stages;

        let mk_builder = |binding: u32| {
            vk::DescriptorSetLayoutBinding::builder()
                .binding(binding)
                .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
                .descriptor_count(1)
                .stage_flags(Stages::COMPUTE)
                .build()
        };

        let nodes = mk_builder(0);
        let node_bins = mk_builder(1);
        let node_bin_offsets = mk_builder(2);
        let bin_offsets = mk_builder(3);
        let bins = mk_builder(4);

        [nodes, node_bins, node_bin_offsets, bin_offsets, bins]
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

pub struct ComputeBuffer {
    pub(super) buffer: vk::Buffer,
    pub(super) memory: vk::DeviceMemory,
    pub(super) size: vk::DeviceSize,

    pub(super) element_count: usize,
}

impl ComputeBuffer {
    pub fn new<T>(app: &GfaestusVk, element_count: usize) -> Result<Self> {
        let size = ((element_count * std::mem::size_of::<T>()) as u32)
            as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::TRANSFER_DST
            | vk::BufferUsageFlags::TRANSFER_SRC
            | vk::BufferUsageFlags::STORAGE_BUFFER;

        let mem_props = vk::MemoryPropertyFlags::DEVICE_LOCAL;

        let (buffer, memory, size) =
            app.create_buffer(size, usage, mem_props)?;

        // let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
        //     | vk::MemoryPropertyFlags::HOST_CACHED
        //     | vk::MemoryPropertyFlags::HOST_COHERENT;

        Ok(Self {
            buffer,
            memory,
            size,

            element_count,
        })
    }
}

pub struct BinBuffers {
    node_bins: ComputeBuffer,
    node_bin_offsets: ComputeBuffer,
    bin_offsets: ComputeBuffer,
    bins: ComputeBuffer,
}

impl BinBuffers {
    fn new(
        app: &GfaestusVk,
        node_count: usize,
        bin_count: usize,
    ) -> Result<Self> {
        // node_bins maps node ends to bin ID, i.e. index in `bins`
        let node_bins = ComputeBuffer::new::<u32>(app, node_count * 2)?;

        // node_bin_offsets maps node ends to offset in bin, in `bins`
        let node_bin_offsets = ComputeBuffer::new::<u32>(app, node_count * 2)?;

        // bin_offsets has the start index and length of each bin in `bins`
        let bin_offsets = ComputeBuffer::new::<u32>(app, bin_count)?;

        // bins has node end index for each bin, in order
        let bins = ComputeBuffer::new::<u32>(app, node_count * 2)?;

        Ok(Self {
            node_bins,
            node_bin_offsets,
            bin_offsets,
            bins,
        })
    }
}

// pub struct ScreenBins {

// }
