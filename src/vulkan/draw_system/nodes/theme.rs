use ash::version::DeviceV1_0;
use ash::{vk, Device};
use rustc_hash::FxHashMap;

use anyhow::Result;

use crate::app::theme::ThemeDef;
use crate::vulkan::texture::Texture1D;
use crate::vulkan::GfaestusVk;

pub struct NodeThemePipeline {
    pub(super) descriptor_pool: vk::DescriptorPool,

    pub(super) descriptor_set_layout: vk::DescriptorSetLayout,

    pub(crate) theme_set: vk::DescriptorSet,
    pub(crate) theme_set_id: usize,

    pub(super) sampler: vk::Sampler,

    pub(super) pipeline_layout: vk::PipelineLayout,
    pub(super) pipeline: vk::Pipeline,

    pub(super) themes: FxHashMap<usize, ThemeData>,

    pub(super) device: Device,
}

impl NodeThemePipeline {
    /// Sets the pipeline to use the theme with the specified ID;
    /// returns None if the theme doesn't exist (or hasn't been
    /// uploaded to the GPU yet), panics if there's
    pub fn set_active_theme(&mut self, theme_id: usize) -> Option<()> {
        if theme_id == self.theme_set_id {
            return Some(());
        }

        let theme = self.themes.get(&theme_id)?;
        self.theme_set_id = theme_id;

        theme
            .write_descriptor_set(&self.device, self.sampler, &self.theme_set)
            .expect(&format!("Error writing theme {} descriptor set", theme_id));

        Some(())
    }

    pub fn active_background_color(&self) -> rgb::RGB<f32> {
        if let Some(theme) = self.themes.get(&self.theme_set_id) {
            theme.background_color
        } else {
            rgb::RGB::new(1.0, 1.0, 1.0)
        }
    }

    fn theme_layout_binding() -> vk::DescriptorSetLayoutBinding {
        use vk::ShaderStageFlags as Stages;

        vk::DescriptorSetLayoutBinding::builder()
            .binding(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .descriptor_count(1)
            .stage_flags(Stages::FRAGMENT)
            .build()
    }

    fn create_descriptor_set_layout(device: &Device) -> Result<vk::DescriptorSetLayout> {
        let binding = Self::theme_layout_binding();
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
            include_bytes!("../../../../shaders/nodes_themed.frag.spv"),
        )
    }

    pub fn new(
        app: &GfaestusVk,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
        selection_set_layout: vk::DescriptorSetLayout,
        // image_count: usize,
    ) -> Result<Self> {
        let device = app.vk_context().device();

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

        let themes = FxHashMap::default();

        Ok(Self {
            descriptor_pool,
            descriptor_set_layout: desc_set_layout,

            theme_set: descriptor_sets[0],
            theme_set_id: std::usize::MAX,

            sampler,

            pipeline_layout,
            pipeline,

            themes,

            device: device.clone(),
        })
    }

    pub fn destroy(&mut self) {
        unsafe {
            for (_ix, theme) in self.themes.iter_mut() {
                theme.destroy(&self.device);
            }
            self.themes.clear();

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

    pub fn has_theme(&self, theme_id: usize) -> bool {
        self.themes.contains_key(&theme_id)
    }

    pub fn upload_theme_data(
        &mut self,
        app: &GfaestusVk,
        theme_id: usize,
        theme_def: &ThemeDef,
    ) -> Result<()> {
        let theme = ThemeData::from_theme_def(app, theme_def)?;

        // handle cleanup if theme already exists
        if let Some(old_theme) = self.themes.get_mut(&theme_id) {
            old_theme.destroy(app.vk_context().device());
        }

        self.themes.insert(theme_id, theme);

        Ok(())
    }

    pub fn destroy_theme(&mut self, theme_id: usize) {
        if let Some(theme) = self.themes.get_mut(&theme_id) {
            theme.destroy(&self.device);
        }
        self.themes.remove(&theme_id);
    }
}

pub struct ThemeData {
    pub texture: Texture1D,
    pub background_color: rgb::RGB<f32>,
}

impl ThemeData {
    pub fn destroy(&mut self, device: &Device) {
        self.texture.destroy(device);
    }

    pub fn from_theme_def(app: &GfaestusVk, theme_def: &ThemeDef) -> Result<Self> {
        let colors = &theme_def.node_colors;

        let texture = Texture1D::create_from_colors(
            app,
            app.transient_command_pool,
            app.graphics_queue,
            &colors,
        )?;

        Ok(Self {
            texture,
            background_color: theme_def.background,
        })
    }

    pub fn write_descriptor_set(
        &self,
        device: &Device,
        sampler: vk::Sampler,
        descriptor_set: &vk::DescriptorSet,
    ) -> Result<()> {
        let image_info = vk::DescriptorImageInfo::builder()
            .image_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .image_view(self.texture.view)
            .sampler(sampler)
            .build();
        let image_infos = [image_info];

        let sampler_descriptor_write = vk::WriteDescriptorSet::builder()
            .dst_set(*descriptor_set)
            .dst_binding(0)
            .dst_array_element(0)
            .descriptor_type(vk::DescriptorType::COMBINED_IMAGE_SAMPLER)
            .image_info(&image_infos)
            .build();

        let descriptor_writes = [sampler_descriptor_write];

        unsafe { device.update_descriptor_sets(&descriptor_writes, &[]) };

        Ok(())
    }
}

const RAINBOW: [(f32, f32, f32); 7] = [
    (1.0, 0.0, 0.0),
    (1.0, 0.65, 0.0),
    (1.0, 1.0, 0.0),
    (0.0, 0.5, 0.0),
    (0.0, 0.0, 1.0),
    (0.3, 0.0, 0.51),
    (0.93, 0.51, 0.93),
];
