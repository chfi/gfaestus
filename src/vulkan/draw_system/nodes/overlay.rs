use ash::version::DeviceV1_0;
use ash::{vk, Device};
use rustc_hash::FxHashMap;

use std::ffi::CString;

use nalgebra_glm as glm;

use anyhow::Result;

use crate::geometry::Point;
use crate::view::View;
use crate::vulkan::GfaestusVk;
use crate::vulkan::SwapchainProperties;

use crate::vulkan::render_pass::Framebuffers;

use super::super::create_shader_module;
use super::super::Vertex;

pub struct NodeOverlayPipeline {
    pub(super) descriptor_pool: vk::DescriptorPool,

    pub(super) descriptor_set_layout: vk::DescriptorSetLayout,

    pub(super) overlay_set: vk::DescriptorSet,
    pub(super) overlay_set_id: Option<usize>,

    pub(super) sampler: vk::Sampler,

    pub(super) pipeline_layout: vk::PipelineLayout,
    pub(super) pipeline: vk::Pipeline,

    pub(super) overlays: FxHashMap<usize, NodeOverlay>,

    pub(super) device: Device,
}

impl NodeOverlayPipeline {
    pub fn set_active_overlay(&mut self, overlay_id: Option<usize>) -> Option<()> {
        if let Some(cur_id) = self.overlay_set_id {
            if Some(cur_id) == overlay_id {
                return Some(());
            }
        } else {
            self.overlay_set_id = None;
            return Some(());
        }

        let overlay_id = overlay_id?;

        let overlay = self.overlays.get(&overlay_id)?;
        self.overlay_set_id = Some(overlay_id);

        overlay
            .write_descriptor_set(&self.device, self.sampler, &self.overlay_set)
            .expect(&format!(
                "Error writing theme {} descriptor set",
                overlay_id
            ));

        Some(())
    }

    fn overlay_layout_binding() -> vk::DescriptorSetLayoutBinding {
        use vk::ShaderStageFlags as Stages;

        vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_TEXEL_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::FRAGMENT)
            .build()
    }

    fn create_descriptor_set_layout(device: &Device) -> Result<vk::DescriptorSetLayout> {
        let binding = Self::overlay_layout_binding();
        let bindings = [binding];

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings)
            .build();

        let layout = unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

        Ok(layout)
    }

    fn create_pipeline(
        device: &Device,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
        descriptor_set_layout: vk::DescriptorSetLayout,
        selection_set_layout: vk::DescriptorSetLayout,
    ) -> (vk::Pipeline, vk::PipelineLayout) {
        super::create_pipeline(
            device,
            msaa_samples,
            render_pass,
            &[descriptor_set_layout, selection_set_layout],
            include_bytes!("../../../../shaders/nodes_overlay.frag.spv"),
        )
    }

    pub(super) fn new(
        device: &Device,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
        selection_set_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        let desc_set_layout = Self::create_descriptor_set_layout(device)?;

        let (pipeline, pipeline_layout) = Self::create_pipeline(
            device,
            msaa_samples,
            render_pass,
            desc_set_layout,
            selection_set_layout,
        );

        let sampler = super::create_sampler(device)?;

        let image_count = 1;

        let descriptor_pool = {
            let sampler_pool_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: image_count,
            };

            let pool_sizes = [sampler_pool_size];

            let pool_info = vk::DescriptorPoolCreateInfo::builder()
                .pool_sizes(&pool_sizes)
                .max_sets(image_count)
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

        let overlays = FxHashMap::default();

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout: desc_set_layout,

            overlay_set: descriptor_sets[0],
            overlay_set_id: None,

            overlays,

            sampler,

            pipeline_layout,
            pipeline,

            device: device.clone(),
        })
    }

    pub fn destroy(&mut self) {
        unsafe {
            self.device
                .destroy_descriptor_set_layout(self.descriptor_set_layout, None);
            self.device.destroy_sampler(self.sampler, None);

            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_pipeline(self.pipeline, None);

            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

pub struct NodeOverlay {
    name: String,

    descriptor_set: vk::DescriptorSet,

    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,

    host_visible: bool,
}

impl NodeOverlay {
    /// Create a new overlay that can be written to by the CPU after construction
    ///
    /// Uses host-visible and host-coherent memory
    pub fn new_empty(
        name: &str,
        app: &GfaestusVk,
        pool: vk::DescriptorPool,
        layout: vk::DescriptorSetLayout,
        node_count: usize,
    ) -> Result<Self> {
        let device = app.vk_context().device();

        let size = ((node_count * std::mem::size_of::<[u8; 4]>()) as u32) as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::STORAGE_TEXEL_BUFFER | vk::BufferUsageFlags::TRANSFER_DST;

        let mem_props =
            vk::MemoryPropertyFlags::HOST_VISIBLE | vk::MemoryPropertyFlags::HOST_COHERENT;

        let (buffer, memory, size) = app.create_buffer(size, usage, mem_props)?;

        let descriptor_sets = {
            let layouts = vec![layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        for set in descriptor_sets.iter() {
            let buf_info = vk::DescriptorBufferInfo::builder()
                .buffer(buffer)
                .offset(0)
                .range(vk::WHOLE_SIZE)
                .build();

            let buf_infos = [buf_info];

            let descriptor_write = vk::WriteDescriptorSet::builder()
                .dst_set(*set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::STORAGE_TEXEL_BUFFER)
                .buffer_info(&buf_infos)
                .build();

            let descriptor_writes = [descriptor_write];

            unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) }
        }

        Ok(Self {
            name: name.into(),
            descriptor_set: descriptor_sets[0],

            buffer,
            memory,
            size,

            host_visible: true,
        })
    }

    /// Update the colors for a host-visible overlay by providing a
    /// set of node IDs and new colors
    pub fn update_overlay<I>(&mut self, device: &Device, new_colors: I) -> Result<()>
    where
        I: IntoIterator<Item = (handlegraph::handle::NodeId, rgb::RGB<f32>)>,
    {
        assert!(self.host_visible);

        unsafe {
            let ptr = device.map_memory(self.memory, 0, self.size, vk::MemoryMapFlags::empty())?;

            for (node, color) in new_colors.into_iter() {
                let val_ptr = ptr as *mut u32;
                let ix = (node.0 - 1) as usize;

                let val_ptr = (val_ptr.add(ix)) as *mut u8;
                val_ptr.write((color.r * 255.0) as u8);

                let val_ptr = val_ptr.add(1);
                val_ptr.write((color.g * 255.0) as u8);

                let val_ptr = val_ptr.add(1);
                val_ptr.write((color.b * 255.0) as u8);

                let val_ptr = val_ptr.add(1);
                val_ptr.write(255u8);
            }

            device.unmap_memory(self.memory);
        }

        Ok(())
    }

    /// Create a new overlay that's filled during construction and immutable afterward
    ///
    /// Uses device memory if available
    pub fn new_static<F>(
        name: &str,
        app: &GfaestusVk,
        pool: vk::DescriptorPool,
        layout: vk::DescriptorSetLayout,
        graph: crate::graph_query::GraphQuery,
        mut overlay_fn: F,
    ) -> Result<Self>
    where
        F: FnMut(
            &handlegraph::packedgraph::PackedGraph,
            handlegraph::handle::NodeId,
        ) -> rgb::RGB<f32>,
    {
        use handlegraph::handlegraph::IntoHandles;

        let device = app.vk_context().device();

        let buffer_size = (graph.node_count() * std::mem::size_of::<[u8; 4]>()) as vk::DeviceSize;

        let mut pixels: Vec<u8> = Vec::with_capacity(buffer_size as usize);

        {
            let graph = graph.graph();

            let mut nodes = graph.handles().map(|h| h.id()).collect::<Vec<_>>();

            nodes.sort();

            for node in nodes {
                let color = overlay_fn(graph, node);

                pixels.push((color.r * 255.0) as u8);
                pixels.push((color.g * 255.0) as u8);
                pixels.push((color.b * 255.0) as u8);
                pixels.push(255);
            }
        }

        let (buffer, memory) = app.create_device_local_buffer_with_data::<[u8; 4], _>(
            vk::BufferUsageFlags::TRANSFER_DST | vk::BufferUsageFlags::STORAGE_TEXEL_BUFFER,
            &pixels,
        )?;

        let descriptor_sets = {
            let layouts = vec![layout];

            let alloc_info = vk::DescriptorSetAllocateInfo::builder()
                .descriptor_pool(pool)
                .set_layouts(&layouts)
                .build();

            unsafe { device.allocate_descriptor_sets(&alloc_info) }
        }?;

        for set in descriptor_sets.iter() {
            let buf_info = vk::DescriptorBufferInfo::builder()
                .buffer(buffer)
                .offset(0)
                .range(vk::WHOLE_SIZE)
                .build();

            let buf_infos = [buf_info];

            let descriptor_write = vk::WriteDescriptorSet::builder()
                .dst_set(*set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::STORAGE_TEXEL_BUFFER)
                .buffer_info(&buf_infos)
                .build();

            let descriptor_writes = [descriptor_write];

            unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) }
        }

        Ok(Self {
            name: name.into(),
            descriptor_set: descriptor_sets[0],

            buffer,
            memory,
            size: buffer_size,

            host_visible: false,
        })
    }

    pub fn destroy(&self, device: &Device) {
        unsafe {
            device.destroy_buffer(self.buffer, None);
            device.free_memory(self.memory, None);
        }
    }

    pub fn write_descriptor_set(
        &self,
        device: &Device,
        sampler: vk::Sampler,
        descriptor_set: &vk::DescriptorSet,
    ) -> Result<()> {
        let buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();
        let buf_infos = [buf_info];

        let sampler_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(*descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_TEXEL_BUFFER)
            .buffer_info(&buf_infos)
            .build();

        let descriptor_writes = [sampler_descriptor_write];

        unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) };

        Ok(())
    }
}
