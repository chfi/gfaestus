use ash::version::DeviceV1_0;
use ash::{vk, Device};
use rustc_hash::FxHashMap;

use anyhow::*;

use crate::vulkan::context::NodeRendererType;
use crate::vulkan::texture::GradientTexture;
use crate::{overlays::OverlayKind, vulkan::GfaestusVk};

use super::NodePipelineConfig;

pub struct OverlayPipelines {
    pub pipeline_rgb: OverlayPipelineRGB,
    pub pipeline_value: OverlayPipelineValue,

    pub(super) overlay_set_id: Option<usize>,

    pub(super) overlays: FxHashMap<usize, Overlay>,

    next_overlay_id: usize,

    #[allow(dead_code)]
    pub(super) device: Device,
}

impl OverlayPipelines {
    pub(super) fn new(
        app: &GfaestusVk,
        renderer_type: NodeRendererType,
        selection_set_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        let pipeline_rgb =
            OverlayPipelineRGB::new(app, renderer_type, selection_set_layout)?;
        let pipeline_value = OverlayPipelineValue::new(
            app,
            renderer_type,
            selection_set_layout,
        )?;

        Ok(Self {
            pipeline_rgb,
            pipeline_value,

            overlay_set_id: None,
            overlays: Default::default(),

            next_overlay_id: 0,

            device: app.vk_context().device().clone(),
        })
    }

    pub fn destroy(&self, allocator: &vk_mem::Allocator) -> Result<()> {
        self.pipeline_rgb.destroy();
        self.pipeline_value.destroy();
        for overlay in self.overlays.values() {
            allocator.destroy_buffer(overlay.buffer, &overlay.alloc)?;
        }
        Ok(())
    }

    pub fn overlay_kind(&self, id: usize) -> Option<OverlayKind> {
        let o = self.overlays.get(&id)?;
        Some(o.kind)
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
        overlay_id: usize,
        color_scheme: &GradientTexture,
    ) -> Result<()> {
        let overlay = self.overlays.get(&overlay_id).ok_or(anyhow!(
            "Tried to write nonexistent overlay ID {}",
            overlay_id
        ))?;

        match overlay.kind {
            OverlayKind::RGB => {
                self.pipeline_rgb.write_active_overlay(overlay)?;
            }
            OverlayKind::Value => {
                self.pipeline_value
                    .write_active_overlay(color_scheme, overlay)?;
            }
        }

        self.overlay_set_id = Some(overlay_id);

        Ok(())
    }

    pub(super) fn bind_descriptor_sets(
        &self,
        device: &Device,
        cmd_buf: vk::CommandBuffer,
        overlay_id: usize,
        // overlay: (usize, OverlayKind),
        selection_descriptor: vk::DescriptorSet,
    ) -> Result<()> {
        let overlay = self.overlays.get(&overlay_id).unwrap();

        unsafe {
            let (desc_sets, layout) = match overlay.kind {
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
        let mut overlays = Vec::with_capacity(self.overlays.len());

        overlays.extend(
            self.overlays.iter().map(|(id, overlay)| {
                (*id, overlay.kind, overlay.name.as_str())
            }),
        );

        overlays.sort_by_key(|(id, _, _)| *id);

        overlays
    }

    pub fn create_overlay(&mut self, overlay: Overlay) -> usize {
        let overlay_id = self.next_overlay_id;
        self.next_overlay_id += 1;

        self.update_overlay(overlay_id, overlay);

        overlay_id
    }

    fn update_overlay(&mut self, overlay_id: usize, overlay: Overlay) {
        self.overlays.insert(overlay_id, overlay);
    }
}

pub struct OverlayPipelineRGB {
    pub(super) descriptor_pool: vk::DescriptorPool,
    pub descriptor_set_layout: vk::DescriptorSetLayout,

    pub overlay_set: vk::DescriptorSet,

    pub(super) pipeline_layout: vk::PipelineLayout,
    pub(super) pipeline: vk::Pipeline,

    pub(super) device: Device,
}

pub struct OverlayPipelineValue {
    pub(super) descriptor_pool: vk::DescriptorPool,
    pub descriptor_set_layout: vk::DescriptorSetLayout,

    sampler: vk::Sampler,

    pub overlay_set: vk::DescriptorSet,

    pub(super) pipeline_layout: vk::PipelineLayout,
    pub(super) pipeline: vk::Pipeline,

    pub(super) device: Device,
}

impl OverlayPipelineValue {
    fn write_active_overlay(
        &mut self,
        color_scheme: &GradientTexture,
        overlay: &Overlay,
    ) -> Result<()> {
        overlay.write_value_descriptor_set(
            &self.device,
            color_scheme,
            self.sampler,
            &self.overlay_set,
        )?;

        Ok(())
    }

    fn layout_bindings() -> [vk::DescriptorSetLayoutBinding; 2] {
        use vk::ShaderStageFlags as Stages;

        let sampler = vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(Stages::FRAGMENT | Stages::COMPUTE)
            .build();

        let values = vk::DescriptorSetLayoutBinding::builder()
            .binding(1)
            .descriptor_type(vk::DescriptorType::STORAGE_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::FRAGMENT | Stages::COMPUTE)
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
        app: &GfaestusVk,
        renderer_type: NodeRendererType,
        descriptor_set_layout: vk::DescriptorSetLayout,
        selection_set_layout: vk::DescriptorSetLayout,
    ) -> Result<(vk::Pipeline, vk::PipelineLayout)> {
        let pipeline_config = NodePipelineConfig {
            kind: super::PipelineKind::OverlayU,
        };

        super::create_node_pipeline(
            app,
            renderer_type,
            pipeline_config,
            &[descriptor_set_layout, selection_set_layout],
        )
    }

    pub(super) fn new(
        app: &GfaestusVk,
        renderer_type: NodeRendererType,
        selection_set_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        let device = app.vk_context().device();

        let desc_set_layout = Self::create_descriptor_set_layout(device)?;

        let (pipeline, pipeline_layout) = Self::create_pipeline(
            app,
            renderer_type,
            desc_set_layout,
            selection_set_layout,
        )?;

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

        app.set_debug_object_name(pipeline, "Node Overlay Value Pipeline")?;
        app.set_debug_object_name(
            descriptor_pool,
            "Node Overlay Value - Descriptor Pool",
        )?;
        app.set_debug_object_name(
            descriptor_sets[0],
            "Node Overlay Value - Descriptor Set",
        )?;

        app.set_debug_object_name(sampler, "Node Overlay Value - Sampler")?;

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout: desc_set_layout,

            overlay_set: descriptor_sets[0],

            sampler,

            pipeline_layout,
            pipeline,

            // overlays: Default::default(),
            device: device.clone(),
        })
    }

    pub fn destroy(&self) {
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
    fn write_active_overlay(&mut self, overlay: &Overlay) -> Result<()> {
        overlay.write_rgb_descriptor_set(&self.device, &self.overlay_set)?;

        Ok(())
    }

    fn layout_binding() -> vk::DescriptorSetLayoutBinding {
        use vk::ShaderStageFlags as Stages;

        vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::UNIFORM_TEXEL_BUFFER)
            .descriptor_count(1)
            .stage_flags(Stages::FRAGMENT | Stages::COMPUTE)
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
        app: &GfaestusVk,
        renderer_type: NodeRendererType,
        descriptor_set_layout: vk::DescriptorSetLayout,
        selection_set_layout: vk::DescriptorSetLayout,
    ) -> Result<(vk::Pipeline, vk::PipelineLayout)> {
        let pipeline_config = NodePipelineConfig {
            kind: super::PipelineKind::OverlayRgb,
        };

        super::create_node_pipeline(
            app,
            renderer_type,
            pipeline_config,
            &[descriptor_set_layout, selection_set_layout],
        )
    }

    pub(super) fn new(
        app: &GfaestusVk,
        renderer_type: NodeRendererType,
        selection_set_layout: vk::DescriptorSetLayout,
    ) -> Result<Self> {
        let device = app.vk_context().device();

        let desc_set_layout = Self::create_descriptor_set_layout(device)?;

        let (pipeline, pipeline_layout) = Self::create_pipeline(
            app,
            renderer_type,
            desc_set_layout,
            selection_set_layout,
        )?;

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

        app.set_debug_object_name(pipeline, "Node Overlay RGB Pipeline")?;
        app.set_debug_object_name(
            descriptor_pool,
            "Node Overlay RGB - Descriptor Pool",
        )?;
        app.set_debug_object_name(
            descriptor_sets[0],
            "Node Overlay RGB - Descriptor Set",
        )?;

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout: desc_set_layout,

            overlay_set: descriptor_sets[0],

            pipeline_layout,
            pipeline,

            // overlays: Default::default(),
            device: device.clone(),
        })
    }
    pub fn destroy(&self) {
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

pub struct Overlay {
    pub name: String,
    pub kind: OverlayKind,

    pub buffer: vk::Buffer,
    alloc: vk_mem::Allocation,
    alloc_info: vk_mem::AllocationInfo,

    pub buffer_view: Option<vk::BufferView>,

    host_visible: bool,
}

impl Overlay {
    /// Create a new overlay that can be written to by the CPU after construction
    ///
    /// Uses host-visible and host-coherent memory
    // TODO this needs to be smarter wrt. host coherency/being mapped,
    // but this should be fine for now
    pub fn new_empty_value(
        name: &str,
        app: &GfaestusVk,
        node_count: usize,
    ) -> Result<Self> {
        let usage = vk::BufferUsageFlags::STORAGE_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST;

        let mem_usage = vk_mem::MemoryUsage::CpuToGpu;

        let (buffer, alloc, alloc_info) = app
            .create_uninitialized_buffer::<f32>(
                usage, mem_usage, true, node_count,
            )?;

        let obj_name = format!("Overlay (Value) - {}", name);
        app.set_debug_object_name(buffer, &obj_name)?;

        let kind = OverlayKind::Value;

        Ok(Self {
            name: name.into(),
            kind,

            buffer,
            alloc,
            alloc_info,

            buffer_view: None,

            host_visible: true,
        })
    }

    pub fn new_empty_rgb(
        name: &str,
        app: &GfaestusVk,
        node_count: usize,
    ) -> Result<Self> {
        let usage = vk::BufferUsageFlags::UNIFORM_TEXEL_BUFFER
            | vk::BufferUsageFlags::TRANSFER_DST;

        let mem_usage = vk_mem::MemoryUsage::CpuToGpu;

        let (buffer, alloc, alloc_info) = app
            .create_uninitialized_buffer::<f32>(
                usage, mem_usage, true, node_count,
            )?;

        let obj_name = format!("Overlay (RGB) - {}", name);
        app.set_debug_object_name(buffer, &obj_name)?;

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
            kind: OverlayKind::RGB,

            buffer,
            alloc,
            alloc_info,

            buffer_view: Some(buffer_view),

            host_visible: true,
        })
    }

    /// Update the colors for a host-visible overlay by providing a
    /// set of node IDs and new values
    pub fn update_value_overlay<I>(
        &mut self,
        // device: &Device,
        new_values: I,
    ) -> Result<()>
    where
        I: IntoIterator<Item = (handlegraph::handle::NodeId, f32)>,
    {
        if matches!(self.kind, OverlayKind::RGB) {
            return Err(anyhow!(
                "Tried to update RGB overlay with single-channel colors"
            ));
        }

        assert!(self.host_visible);

        unsafe {
            let ptr = self.alloc_info.get_mapped_data();

            for (node, value) in new_values.into_iter() {
                let val_ptr = ptr as *mut f32;
                let ix = (node.0 - 1) as usize;

                let val_ptr = (val_ptr.add(ix)) as *mut f32;
                val_ptr.write(value);
            }
        }

        Ok(())
    }

    /// Update the colors for a host-visible overlay by providing a
    /// set of node IDs and new colors
    pub fn update_rgb_overlay<I>(
        &mut self,
        // device: &Device,
        new_colors: I,
    ) -> Result<()>
    where
        I: IntoIterator<Item = (handlegraph::handle::NodeId, rgb::RGBA<f32>)>,
    {
        if matches!(self.kind, OverlayKind::Value) {
            return Err(anyhow!(
                "Tried to update single-channel overlay with RGB colors"
            ));
        }

        assert!(self.host_visible);

        unsafe {
            let ptr = self.alloc_info.get_mapped_data();

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
                val_ptr.write((color.a * 255.0) as u8);
            }
        }

        Ok(())
    }

    fn write_value_descriptor_set(
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

    fn write_rgb_descriptor_set(
        &self,
        device: &Device,
        descriptor_set: &vk::DescriptorSet,
    ) -> Result<()> {
        if let Some(buf_view) = self.buffer_view {
            let buf_views = [buf_view];

            let descriptor_write = vk::WriteDescriptorSet::builder()
                .dst_set(*descriptor_set)
                .dst_binding(0)
                .dst_array_element(0)
                .descriptor_type(vk::DescriptorType::UNIFORM_TEXEL_BUFFER)
                .texel_buffer_view(&buf_views)
                .build();

            let descriptor_writes = [descriptor_write];

            unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) };
        } else {
            log::warn!("RGB overlay is missing buffer view");
        }

        Ok(())
    }
}
