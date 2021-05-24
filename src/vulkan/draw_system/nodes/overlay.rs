use ash::version::DeviceV1_0;
use ash::{vk, Device};
use rustc_hash::FxHashMap;

use anyhow::Result;

use crate::{
    geometry::Point,
    view::View,
    vulkan::{
        render_pass::Framebuffers,
        texture::{GradientTexture, Texture1D},
    },
};
use crate::{overlays::OverlayKind, vulkan::GfaestusVk};

pub struct OverlayPipelines {
    pipeline_rgb: OverlayPipelineRGB,
    pipeline_value: OverlayPipelineValue,

    pub(super) overlay_set_id: Option<(usize, OverlayKind)>,

    next_overlay_id: usize,

    pub(super) device: Device,
}

impl OverlayPipelines {
    pub(super) fn new(
        device: &Device,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
        selection_set_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        let pipeline_rgb = OverlayPipelineRGB::new(
            device,
            msaa_samples,
            render_pass,
            selection_set_layout,
        )?;
        let pipeline_value = OverlayPipelineValue::new(
            device,
            msaa_samples,
            render_pass,
            selection_set_layout,
        )?;

        Ok(Self {
            pipeline_rgb,
            pipeline_value,

            overlay_set_id: None,

            next_overlay_id: 0,

            device: device.clone(),
        })
    }

    pub(super) fn bind_pipeline(
        &self,
        device: &Device,
        cmd_buf: vk::CommandBuffer,
        overlay_kind: OverlayKind,
    ) {
        unsafe {
            match overlay_kind {
                OverlayKind::RGB => device.cmd_bind_pipeline(
                    cmd_buf,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.pipeline_rgb.pipeline,
                ),
                OverlayKind::Value => device.cmd_bind_pipeline(
                    cmd_buf,
                    vk::PipelineBindPoint::GRAPHICS,
                    self.pipeline_value.pipeline,
                ),
            }
        };
    }

    pub(super) fn pipeline_layout_kind(
        &self,
        overlay_kind: OverlayKind,
    ) -> vk::PipelineLayout {
        match overlay_kind {
            OverlayKind::RGB => self.pipeline_rgb.pipeline_layout,
            OverlayKind::Value => self.pipeline_value.pipeline_layout,
        }
    }

    pub(super) fn write_overlay(
        &mut self,
        overlay: (usize, OverlayKind),
        color_scheme: &GradientTexture,
    ) -> Result<()> {
        if self.overlay_set_id != Some(overlay) {
            match overlay.1 {
                OverlayKind::RGB => {
                    self.pipeline_rgb.write_active_overlay(overlay.0)?;
                }
                OverlayKind::Value => {
                    self.pipeline_value
                        .write_active_overlay(color_scheme, overlay.0)?;
                }
            }
            self.overlay_set_id = Some(overlay);
        }

        Ok(())
    }

    pub(super) fn bind_descriptor_sets(
        &self,
        device: &Device,
        cmd_buf: vk::CommandBuffer,
        overlay: (usize, OverlayKind),
        selection_descriptor: vk::DescriptorSet,
    ) -> Result<()> {
        unsafe {
            let (desc_sets, layout) = match overlay.1 {
                OverlayKind::RGB => {
                    let sets =
                        [self.pipeline_rgb.overlay_set, selection_descriptor];
                    let layout = self.pipeline_rgb.pipeline_layout;
                    (sets, layout)
                }
                OverlayKind::Value => {
                    let sets =
                        [self.pipeline_value.overlay_set, selection_descriptor];
                    let layout = self.pipeline_value.pipeline_layout;
                    (sets, layout)
                }
            };

            let null = [];
            device.cmd_bind_descriptor_sets(
                cmd_buf,
                vk::PipelineBindPoint::GRAPHICS,
                layout,
                0,
                &desc_sets[0..=1],
                &null,
            );
        }

        Ok(())
    }

    pub fn overlay_names(&self) -> Vec<(usize, OverlayKind, &str)> {
        let mut overlays = Vec::with_capacity(
            self.pipeline_rgb.overlays.len()
                + self.pipeline_value.overlays.len(),
        );

        overlays.extend(self.pipeline_rgb.overlays.iter().map(
            |(id, overlay)| (*id, OverlayKind::RGB, overlay.name.as_str()),
        ));

        overlays.extend(self.pipeline_value.overlays.iter().map(
            |(id, overlay)| (*id, OverlayKind::Value, overlay.name.as_str()),
        ));

        overlays.sort_by_key(|(id, _, _)| *id);

        overlays
    }

    pub fn create_overlay(&mut self, overlay: Overlay) -> usize {
        let overlay_id = self.next_overlay_id;
        self.next_overlay_id += 1;

        match overlay {
            Overlay::RGB(o) => self.update_rgb_overlay(overlay_id, o),
            Overlay::Value(o) => self.update_value_overlay(overlay_id, o),
        }

        overlay_id
    }

    fn update_rgb_overlay(&mut self, overlay_id: usize, overlay: NodeOverlay) {
        if self.pipeline_value.overlays.contains_key(&overlay_id) {
            panic!("Tried to update a Value overlay ID with an RGB overlay");
        }

        self.pipeline_rgb.overlays.insert(overlay_id, overlay);
    }

    fn update_value_overlay(
        &mut self,
        overlay_id: usize,
        overlay: NodeOverlayValue,
    ) {
        if self.pipeline_rgb.overlays.contains_key(&overlay_id) {
            panic!("Tried to update an RGB overlay ID with a Value overlay");
        }

        self.pipeline_value.overlays.insert(overlay_id, overlay);
    }
}

pub struct OverlayPipelineRGB {
    pub(super) descriptor_pool: vk::DescriptorPool,
    pub(super) descriptor_set_layout: vk::DescriptorSetLayout,

    pub(super) overlay_set: vk::DescriptorSet,

    pub(super) pipeline_layout: vk::PipelineLayout,
    pub(super) pipeline: vk::Pipeline,

    pub(super) overlays: FxHashMap<usize, NodeOverlay>,

    pub(super) device: Device,
}

pub struct OverlayPipelineValue {
    pub(super) descriptor_pool: vk::DescriptorPool,
    pub(super) descriptor_set_layout: vk::DescriptorSetLayout,

    sampler: vk::Sampler,

    pub(super) overlay_set: vk::DescriptorSet,

    pub(super) pipeline_layout: vk::PipelineLayout,
    pub(super) pipeline: vk::Pipeline,

    pub(super) overlays: FxHashMap<usize, NodeOverlayValue>,

    pub(super) device: Device,
}

impl OverlayPipelineValue {
    fn write_active_overlay(
        &mut self,
        color_scheme: &GradientTexture,
        overlay_id: usize,
    ) -> Result<()> {
        if let Some(overlay) = self.overlays.get(&overlay_id) {
            overlay.write_descriptor_set(
                &self.device,
                color_scheme,
                self.sampler,
                &self.overlay_set,
            )?;
        }

        Ok(())
    }

    fn layout_bindings() -> [vk::DescriptorSetLayoutBinding; 2] {
        use vk::ShaderStageFlags as Stages;

        let sampler = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(Stages::FRAGMENT)
            .build();

        let values = vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::FRAGMENT)
            .build();

        [sampler, values]
    }

    fn create_descriptor_set_layout(
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout> {
        let bindings = Self::layout_bindings();

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings)
            .build();

        let layout =
            unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

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
            crate::include_shader!("nodes/overlay_value.frag.spv"),
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

        let image_count = 1;

        let descriptor_pool = {
            let sampler_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::COMBINED_IMAGE_SAMPLER,
                descriptor_count: image_count,
            };

            let value_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::STORAGE_BUFFER,
                descriptor_count: image_count,
            };

            let pool_sizes = [sampler_size, value_size];

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

        let sampler = GradientTexture::create_sampler(device)?;

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout: desc_set_layout,

            overlay_set: descriptor_sets[0],

            sampler,

            pipeline_layout,
            pipeline,

            overlays: Default::default(),

            device: device.clone(),
        })
    }

    pub fn destroy(&mut self) {
        unsafe {
            self.device.destroy_descriptor_set_layout(
                self.descriptor_set_layout,
                None,
            );

            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_pipeline(self.pipeline, None);

            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

impl OverlayPipelineRGB {
    fn write_active_overlay(&mut self, overlay_id: usize) -> Result<()> {
        if let Some(overlay) = self.overlays.get(&overlay_id) {
            overlay.write_descriptor_set(&self.device, &self.overlay_set)?;
        }

        Ok(())
    }

    fn layout_binding() -> vk::DescriptorSetLayoutBinding {
        use vk::ShaderStageFlags as Stages;

        vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_TEXEL_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::FRAGMENT)
            .build()
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
            crate::include_shader!("nodes/overlay_rgb.frag.spv"),
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

        let image_count = 1;

        let descriptor_pool = {
            let pool_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
                descriptor_count: image_count,
            };

            let pool_sizes = [pool_size];

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

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout: desc_set_layout,

            overlay_set: descriptor_sets[0],

            pipeline_layout,
            pipeline,

            overlays: Default::default(),

            device: device.clone(),
        })
    }
    pub fn destroy(&mut self) {
        unsafe {
            self.device.destroy_descriptor_set_layout(
                self.descriptor_set_layout,
                None,
            );

            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_pipeline(self.pipeline, None);

            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

pub struct NodeOverlayPipeline {
    pub(super) descriptor_pool: vk::DescriptorPool,

    pub(super) descriptor_set_layout: vk::DescriptorSetLayout,

    pub(super) overlay_set: vk::DescriptorSet,
    pub(super) overlay_set_id: Option<usize>,

    pub(super) pipeline_layout: vk::PipelineLayout,
    pub(super) pipeline: vk::Pipeline,

    pub(super) overlays: FxHashMap<usize, NodeOverlay>,

    pub(super) device: Device,
}

impl NodeOverlayPipeline {
    pub fn overlay_names(&self) -> impl Iterator<Item = (usize, &str)> + '_ {
        self.overlays.iter().map(|(id, ov)| (*id, ov.name.as_str()))
    }

    pub fn set_active_overlay(
        &mut self,
        overlay_id: Option<usize>,
    ) -> Option<()> {
        if overlay_id.is_none() {
            self.overlay_set_id = None;
            return Some(());
        }

        let overlay_id = overlay_id?;

        if Some(overlay_id) == self.overlay_set_id {
            return Some(());
        }

        let overlay = self.overlays.get(&overlay_id)?;
        self.overlay_set_id = Some(overlay_id);

        overlay
            .write_descriptor_set(&self.device, &self.overlay_set)
            .expect(&format!(
                "Error writing theme {} descriptor set",
                overlay_id
            ));

        Some(())
    }

    pub fn update_overlay(&mut self, overlay_id: usize, overlay: NodeOverlay) {
        self.overlays.insert(overlay_id, overlay);
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

    fn create_descriptor_set_layout(
        device: &Device,
    ) -> Result<vk::DescriptorSetLayout> {
        let binding = Self::overlay_layout_binding();
        let bindings = [binding];

        let layout_info = vk::DescriptorSetLayoutCreateInfo::builder()
            .bindings(&bindings)
            .build();

        let layout =
            unsafe { device.create_descriptor_set_layout(&layout_info, None) }?;

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
            crate::include_shader!("nodes/overlay_rgb.frag.spv"),
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

        let image_count = 1;

        let descriptor_pool = {
            let pool_size = vk::DescriptorPoolSize {
                ty: vk::DescriptorType::UNIFORM_TEXEL_BUFFER,
                descriptor_count: image_count,
            };

            let pool_sizes = [pool_size];

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

            pipeline_layout,
            pipeline,

            device: device.clone(),
        })
    }

    pub fn destroy(&mut self) {
        unsafe {
            self.device.destroy_descriptor_set_layout(
                self.descriptor_set_layout,
                None,
            );

            self.device
                .destroy_pipeline_layout(self.pipeline_layout, None);
            self.device.destroy_pipeline(self.pipeline, None);

            self.device
                .destroy_descriptor_pool(self.descriptor_pool, None);
        }
    }
}

pub enum Overlay {
    RGB(NodeOverlay),
    Value(NodeOverlayValue),
}

pub struct NodeOverlayValue {
    name: String,

    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,

    host_visible: bool,
}

impl NodeOverlayValue {
    /// Create a new overlay that can be written to by the CPU after construction
    ///
    /// Uses host-visible and host-coherent memory
    pub fn new_empty_value(
        name: &str,
        app: &GfaestusVk,
        node_count: usize,
    ) -> Result<Self> {
        let size = ((node_count * std::mem::size_of::<f32>()) as u32)
            as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST;

        let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
            | vk::MemoryPropertyFlags::HOST_COHERENT;

        let (buffer, memory, size) =
            app.create_buffer(size, usage, mem_props)?;

        let device = app.vk_context().device();

        Ok(Self {
            name: name.into(),

            buffer,
            memory,
            size,

            host_visible: true,
        })
    }

    /// Update the colors for a host-visible overlay by providing a
    /// set of node IDs and new colors
    pub fn update_overlay<I>(
        &mut self,
        device: &Device,
        new_values: I,
    ) -> Result<()>
    where
        I: IntoIterator<Item = (handlegraph::handle::NodeId, f32)>,
    {
        assert!(self.host_visible);

        unsafe {
            let ptr = device.map_memory(
                self.memory,
                0,
                self.size,
                vk::MemoryMapFlags::empty(),
            )?;

            for (node, value) in new_values.into_iter() {
                let val_ptr = ptr as *mut f32;
                let ix = (node.0 - 1) as usize;

                let val_ptr = (val_ptr.add(ix)) as *mut f32;
                val_ptr.write(value);
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
        graph: &crate::graph_query::GraphQuery,
        mut overlay_fn: F,
    ) -> Result<Self>
    where
        F: FnMut(
            &handlegraph::packedgraph::PackedGraph,
            handlegraph::handle::NodeId,
        ) -> f32,
    {
        use handlegraph::handlegraph::IntoHandles;

        let device = app.vk_context().device();

        let buffer_size =
            (graph.node_count() * std::mem::size_of::<f32>()) as vk::DeviceSize;

        let mut values: Vec<f32> = Vec::with_capacity(buffer_size as usize);

        {
            let graph = graph.graph();

            let mut nodes = graph.handles().map(|h| h.id()).collect::<Vec<_>>();

            nodes.sort();

            for node in nodes {
                let value = overlay_fn(graph, node);
                values.push(value);
            }
        }

        let (buffer, memory) = app
            .create_device_local_buffer_with_data::<f32, _>(
                vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::STORAGE_BUFFER,
                &values,
            )?;

        Ok(Self {
            name: name.into(),

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
        color_scheme: &GradientTexture,
        sampler: vk::Sampler,
        descriptor_set: &vk::DescriptorSet,
    ) -> Result<()> {
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(color_scheme.texture.view)
            .sampler(sampler)
            .build();
        let image_infos = [image_info];

        let sampler_write = vk::WriteDescriptorSet::builder()
            .dst_set(*descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_infos)
            .build();

        let buf_info = vk::DescriptorBufferInfo::builder()
            .buffer(self.buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .build();

        let buf_infos = [buf_info];

        let values_write = vk::WriteDescriptorSet::builder()
            .dst_set(*descriptor_set)
            .dst_binding(1)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .buffer_info(&buf_infos)
            .build();

        let descriptor_writes = [sampler_write, values_write];

        unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) };

        Ok(())
    }
}

pub struct NodeOverlay {
    name: String,

    buffer: vk::Buffer,
    memory: vk::DeviceMemory,
    size: vk::DeviceSize,

    buffer_view: vk::BufferView,

    host_visible: bool,
}

impl NodeOverlay {
    /// Create a new overlay that can be written to by the CPU after construction
    ///
    /// Uses host-visible and host-coherent memory
    pub fn new_empty_rgb(
        name: &str,
        app: &GfaestusVk,
        node_count: usize,
    ) -> Result<Self> {
        let size = ((node_count * std::mem::size_of::<[u8; 4]>()) as u32)
            as vk::DeviceSize;

        let usage = vk::BufferUsageFlags::UNIFORM_TEXEL_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST;

        let mem_props = vk::MemoryPropertyFlags::HOST_VISIBLE
            | vk::MemoryPropertyFlags::HOST_COHERENT;

        let (buffer, memory, size) =
            app.create_buffer(size, usage, mem_props)?;

        let bufview_info = vk::BufferViewCreateInfo::builder()
            .buffer(buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .format(vk::Format::R8G8B8A8_UNORM)
            .build();

        let device = app.vk_context().device();

        let buffer_view =
            unsafe { device.create_buffer_view(&bufview_info, None) }?;

        Ok(Self {
            name: name.into(),

            buffer,
            memory,
            size,

            buffer_view,

            host_visible: true,
        })
    }

    /// Update the colors for a host-visible overlay by providing a
    /// set of node IDs and new colors
    pub fn update_overlay<I>(
        &mut self,
        device: &Device,
        new_colors: I,
    ) -> Result<()>
    where
        I: IntoIterator<Item = (handlegraph::handle::NodeId, rgb::RGB<f32>)>,
    {
        assert!(self.host_visible);

        unsafe {
            let ptr = device.map_memory(
                self.memory,
                0,
                self.size,
                vk::MemoryMapFlags::empty(),
            )?;

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
        graph: &crate::graph_query::GraphQuery,
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

        let buffer_size = (graph.node_count() * std::mem::size_of::<[u8; 4]>())
            as vk::DeviceSize;

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

        let (buffer, memory) = app
            .create_device_local_buffer_with_data::<[u8; 4], _>(
                vk::BufferUsageFlags::TRANSFER_DST
                    | vk::BufferUsageFlags::UNIFORM_TEXEL_BUFFER,
                &pixels,
            )?;

        let bufview_info = vk::BufferViewCreateInfo::builder()
            .buffer(buffer)
            .offset(0)
            .range(vk::WHOLE_SIZE)
            .format(vk::Format::R8G8B8A8_UNORM)
            .build();

        let buffer_view =
            unsafe { device.create_buffer_view(&bufview_info, None) }?;

        Ok(Self {
            name: name.into(),

            buffer,
            memory,
            size: buffer_size,

            buffer_view,

            host_visible: false,
        })
    }

    pub fn destroy(&self, device: &Device) {
        unsafe {
            device.destroy_buffer_view(self.buffer_view, None);
            device.destroy_buffer(self.buffer, None);
            device.free_memory(self.memory, None);
        }
    }

    pub fn write_descriptor_set(
        &self,
        device: &Device,
        descriptor_set: &vk::DescriptorSet,
    ) -> Result<()> {
        let buf_views = [self.buffer_view];

        let descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(*descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_TEXEL_BUFFER)
            .texel_buffer_view(&buf_views)
            .build();

        let descriptor_writes = [descriptor_write];

        unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) };

        Ok(())
    }
}
