use ash::version::DeviceV1_0;
use ash::{vk, Device};

use super::context::VkContext;
use super::SwapchainProperties;
use super::{texture::*, GfaestusVk};

use anyhow::*;

pub struct RenderPasses {
    pub nodes: vk::RenderPass,
    pub edges: vk::RenderPass,
    pub selection_edge_detect: vk::RenderPass,
    pub selection_blur: vk::RenderPass,
    pub gui: vk::RenderPass,

    pub id_format: vk::Format,
}

pub struct Framebuffers {
    pub nodes: vk::Framebuffer,
    pub edges: vk::Framebuffer,
    pub selection_edge_detect: vk::Framebuffer,
    pub selection_blur: vk::Framebuffer,
    pub gui: vk::Framebuffer,
}

impl Framebuffers {
    pub fn set_vk_debug_names(&self, app: &GfaestusVk) -> Result<()> {
        app.set_debug_object_name(self.nodes, "Node Framebuffer")?;
        app.set_debug_object_name(self.edges, "Edge Framebuffer")?;
        app.set_debug_object_name(
            self.selection_edge_detect,
            "Selection Border Edge Framebuffer",
        )?;
        app.set_debug_object_name(
            self.selection_blur,
            "Selection Border Blur Framebuffer",
        )?;
        app.set_debug_object_name(self.gui, "GUI Framebuffer")?;

        Ok(())
    }

    pub fn destroy(&self, device: &Device) {
        unsafe {
            device.destroy_framebuffer(self.nodes, None);
            device.destroy_framebuffer(self.edges, None);
            device.destroy_framebuffer(self.selection_edge_detect, None);
            device.destroy_framebuffer(self.selection_blur, None);
            device.destroy_framebuffer(self.gui, None);
        }
    }
}

pub struct NodeAttachments {
    pub color: Texture,
    pub resolve: Texture,
    pub id_color: Texture,
    pub id_resolve: Texture,
    pub mask: Texture,
    pub mask_resolve: Texture,

    pub id_format: vk::Format,
}

pub struct OffscreenAttachment {
    pub color: Texture,
}

impl OffscreenAttachment {
    pub fn new(
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        swapchain_props: SwapchainProperties,
    ) -> Result<Self> {
        let format = vk::Format::R8G8B8A8_UNORM;

        let color = Self::color(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
            format,
        )?;

        Ok(Self { color })
    }

    pub fn recreate(
        &mut self,
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        swapchain_props: SwapchainProperties,
    ) -> Result<()> {
        self.destroy(vk_context.device());

        let format = vk::Format::R8G8B8A8_UNORM;

        self.color = Self::color(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
            format,
        )?;

        Ok(())
    }

    pub fn destroy(&mut self, device: &Device) {
        self.color.destroy(device);
    }

    fn color(
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        swapchain_props: SwapchainProperties,
        format: vk::Format,
    ) -> Result<Texture> {
        let extent = swapchain_props.extent;

        let sampler = {
            let sampler_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .anisotropy_enable(false)
                .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
                .unnormalized_coordinates(false)
                .compare_enable(false)
                .compare_op(vk::CompareOp::ALWAYS)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .mip_lod_bias(0.0)
                .min_lod(0.0)
                .max_lod(1.0)
                .build();

            unsafe { vk_context.device().create_sampler(&sampler_info, None) }
        }?;

        let color = Texture::create_attachment_image(
            vk_context,
            command_pool,
            queue,
            vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            extent,
            format,
            Some(sampler),
        )?;

        Ok(color)
    }
}

impl NodeAttachments {
    pub fn new(
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
        id_format: vk::Format,
    ) -> Result<Self> {
        let color = Self::color(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
            msaa_samples,
        )?;

        let resolve =
            Self::resolve(vk_context, command_pool, queue, swapchain_props)?;

        let id_color = Self::id_color(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
            msaa_samples,
            id_format,
        )?;

        let id_resolve = Self::id_resolve(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
            id_format,
        )?;

        let mask = Self::mask(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
            msaa_samples,
        )?;

        let mask_resolve = Self::mask_resolve(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
        )?;

        Ok(Self {
            color,
            resolve,
            id_resolve,
            id_color,
            mask,
            mask_resolve,

            id_format,
        })
    }

    pub fn recreate(
        &mut self,
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
    ) -> Result<()> {
        self.destroy(vk_context.device());

        self.color = Self::color(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
            msaa_samples,
        )?;
        self.resolve =
            Self::resolve(vk_context, command_pool, queue, swapchain_props)?;

        self.id_color = Self::id_color(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
            msaa_samples,
            self.id_format,
        )?;
        self.id_resolve = Self::id_resolve(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
            self.id_format,
        )?;

        self.mask = Self::mask(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
            msaa_samples,
        )?;
        self.mask_resolve = Self::mask_resolve(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
        )?;

        Ok(())
    }

    pub fn set_vk_debug_names(&self, app: &GfaestusVk) -> Result<()> {
        app.set_debug_object_name(self.color.image, "Node Attch. Color")?;
        app.set_debug_object_name(
            self.resolve.image,
            "Node Attch. Color Resolve",
        )?;

        app.set_debug_object_name(self.id_color.image, "Node Attch. ID Image")?;
        app.set_debug_object_name(
            self.id_resolve.image,
            "Node Attch. ID Resolve",
        )?;

        app.set_debug_object_name(self.mask.image, "Node Attch. Mask Color")?;
        app.set_debug_object_name(
            self.mask_resolve.image,
            "Node Attch. Mask Resolve",
        )?;

        Ok(())
    }

    pub fn destroy(&mut self, device: &Device) {
        self.color.destroy(device);
        self.resolve.destroy(device);
        self.id_color.destroy(device);
        self.id_resolve.destroy(device);
        self.mask.destroy(device);
        self.mask_resolve.destroy(device);
    }

    fn color(
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
    ) -> Result<Texture> {
        let color = Texture::create_transient_color(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
            msaa_samples,
        )?;

        Ok(color)
    }

    fn resolve(
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        swapchain_props: SwapchainProperties,
    ) -> Result<Texture> {
        let extent = swapchain_props.extent;

        let resolve = Texture::create_attachment_image(
            vk_context,
            command_pool,
            queue,
            vk::ImageUsageFlags::COLOR_ATTACHMENT,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            extent,
            swapchain_props.format.format,
            None,
        )?;

        Ok(resolve)
    }

    fn id_color(
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
        id_format: vk::Format,
    ) -> Result<Texture> {
        let format = id_format;

        use vk::ImageLayout as Layout;
        use vk::ImageUsageFlags as Usage;

        let (img, mem) = super::GfaestusVk::create_image(
            vk_context,
            vk::MemoryPropertyFlags::DEVICE_LOCAL,
            swapchain_props.extent,
            msaa_samples,
            format,
            vk::ImageTiling::OPTIMAL,
            Usage::TRANSIENT_ATTACHMENT | Usage::COLOR_ATTACHMENT,
        )?;

        super::GfaestusVk::transition_image(
            vk_context.device(),
            command_pool,
            queue,
            img,
            Layout::UNDEFINED,
            Layout::COLOR_ATTACHMENT_OPTIMAL,
        )?;

        let view = super::GfaestusVk::create_image_view(
            vk_context.device(),
            img,
            1,
            format,
            vk::ImageAspectFlags::COLOR,
        )?;

        Ok(Texture::new(img, mem, view, None))
    }

    fn id_resolve(
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        swapchain_props: SwapchainProperties,
        id_format: vk::Format,
    ) -> Result<Texture> {
        let extent = swapchain_props.extent;

        let color = Texture::create_attachment_image(
            vk_context,
            command_pool,
            queue,
            vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_SRC,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            extent,
            id_format,
            None,
        )?;

        Ok(color)
    }

    fn mask(
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
    ) -> Result<Texture> {
        let mut props = swapchain_props;
        props.format.format = vk::Format::R8G8B8A8_UNORM;
        let color = Texture::create_transient_color(
            vk_context,
            command_pool,
            queue,
            props,
            msaa_samples,
        )?;

        Ok(color)
    }

    fn mask_resolve(
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        swapchain_props: SwapchainProperties,
    ) -> Result<Texture> {
        let device = vk_context.device();
        let extent = swapchain_props.extent;

        let mask_sampler = {
            let sampler_info = vk::SamplerCreateInfo::builder()
                .mag_filter(vk::Filter::LINEAR)
                .min_filter(vk::Filter::LINEAR)
                .address_mode_u(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_v(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                .anisotropy_enable(false)
                .border_color(vk::BorderColor::INT_OPAQUE_BLACK)
                .unnormalized_coordinates(false)
                .compare_enable(false)
                .compare_op(vk::CompareOp::ALWAYS)
                .mipmap_mode(vk::SamplerMipmapMode::LINEAR)
                .mip_lod_bias(0.0)
                .min_lod(0.0)
                .max_lod(1.0)
                .build();

            unsafe { device.create_sampler(&sampler_info, None) }
        }?;

        let mask = Texture::create_attachment_image(
            vk_context,
            command_pool,
            queue,
            vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::SAMPLED,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL,
            extent,
            vk::Format::R8G8B8A8_UNORM,
            Some(mask_sampler),
        )?;

        Ok(mask)
    }
}

impl RenderPasses {
    pub fn create(
        vk_context: &VkContext,
        device: &Device,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
    ) -> Result<Self> {
        let id_format: vk::Format = {
            // TODO what other possibilities are compatible?
            let candidates = [
                vk::Format::R32_UINT,
                vk::Format::R32G32_UINT,
                vk::Format::R32G32B32_UINT,
                vk::Format::R32G32B32A32_UINT,
            ];

            let tiling = vk::ImageTiling::OPTIMAL;

            let features = vk::FormatFeatureFlags::TRANSFER_SRC
                | vk::FormatFeatureFlags::COLOR_ATTACHMENT;

            let format =
                vk_context.find_supported_format(&candidates, tiling, features);

            format.ok_or(anyhow!(
                "Could not find a format for the node ID image"
            ))?
        };

        log::debug!("Chose node ID image format: {:?}", id_format);

        let nodes = Self::create_nodes(
            device,
            swapchain_props,
            msaa_samples,
            id_format,
        )?;
        let edges = Self::create_edges(device, swapchain_props, msaa_samples)?;
        let selection_edge_detect = Self::create_selection_edge_detect(
            device,
            vk::Format::R8G8B8A8_UNORM,
        )?;
        let selection_blur =
            Self::create_selection_blur(device, swapchain_props)?;
        let gui = Self::create_gui(device, swapchain_props)?;

        Ok(Self {
            nodes,
            edges,
            selection_edge_detect,
            selection_blur,
            gui,

            id_format,
        })
    }

    pub fn framebuffers(
        &self,
        device: &Device,
        node_attachments: &NodeAttachments,
        offscreen_attachment: &OffscreenAttachment,
        swapchain_image_view: vk::ImageView,
        swapchain_props: SwapchainProperties,
    ) -> Result<Framebuffers> {
        let extent = swapchain_props.extent;

        let nodes = {
            let attachments = [
                // color attachments
                node_attachments.color.view,
                node_attachments.id_color.view,
                node_attachments.mask.view,
                //
                // resolve attachments
                // node_attachments.resolve.view,
                swapchain_image_view,
                node_attachments.id_resolve.view,
                node_attachments.mask_resolve.view,
            ];

            let framebuffer_info = vk::FramebufferCreateInfo::builder()
                .render_pass(self.nodes)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1)
                .build();

            unsafe { device.create_framebuffer(&framebuffer_info, None) }
        }?;

        let edges = {
            let attachments = [
                // color attachments
                node_attachments.color.view,
                // resolve attachments
                // node_attachments.resolve.view,
                swapchain_image_view,
            ];

            let framebuffer_info = vk::FramebufferCreateInfo::builder()
                .render_pass(self.edges)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1)
                .build();

            unsafe { device.create_framebuffer(&framebuffer_info, None) }
        }?;

        let selection_edge_detect = {
            let attachments = [offscreen_attachment.color.view];

            let framebuffer_info = vk::FramebufferCreateInfo::builder()
                .render_pass(self.selection_edge_detect)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1)
                .build();

            unsafe { device.create_framebuffer(&framebuffer_info, None) }
        }?;

        let selection_blur = {
            let attachments = [swapchain_image_view];

            let framebuffer_info = vk::FramebufferCreateInfo::builder()
                .render_pass(self.selection_blur)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1)
                .build();

            unsafe { device.create_framebuffer(&framebuffer_info, None) }
        }?;

        let gui = {
            let attachments = [swapchain_image_view];

            let framebuffer_info = vk::FramebufferCreateInfo::builder()
                .render_pass(self.gui)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1)
                .build();

            unsafe { device.create_framebuffer(&framebuffer_info, None) }
        }?;

        Ok(Framebuffers {
            nodes,
            edges,
            selection_edge_detect,
            selection_blur,
            gui,
        })
    }

    pub fn set_vk_debug_names(&self, app: &GfaestusVk) -> Result<()> {
        app.set_debug_object_name(self.nodes, "Node Render Pass")?;
        app.set_debug_object_name(self.edges, "Edge Render Pass")?;
        app.set_debug_object_name(
            self.selection_edge_detect,
            "Selection Border Edge Detect Render Pass",
        )?;
        app.set_debug_object_name(
            self.selection_blur,
            "Selection Border Blur Render Pass",
        )?;
        app.set_debug_object_name(self.gui, "GUI Render Pass")?;

        Ok(())
    }

    pub fn recreate(
        &mut self,
        device: &Device,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
    ) -> Result<()> {
        self.destroy(device);

        let nodes = Self::create_nodes(
            device,
            swapchain_props,
            msaa_samples,
            self.id_format,
        )?;
        let edges = Self::create_edges(device, swapchain_props, msaa_samples)?;

        let selection_edge_detect = Self::create_selection_edge_detect(
            device,
            vk::Format::R8G8B8A8_UNORM,
        )?;
        let selection_blur =
            Self::create_selection_blur(device, swapchain_props)?;
        let gui = Self::create_gui(device, swapchain_props)?;

        self.nodes = nodes;
        self.edges = edges;
        self.selection_edge_detect = selection_edge_detect;
        self.selection_blur = selection_blur;
        self.gui = gui;

        Ok(())
    }

    pub fn destroy(&self, device: &Device) {
        unsafe {
            device.destroy_render_pass(self.nodes, None);
            device.destroy_render_pass(self.edges, None);
            device.destroy_render_pass(self.selection_edge_detect, None);
            device.destroy_render_pass(self.selection_blur, None);
            device.destroy_render_pass(self.gui, None);
        }
    }

    fn create_edges(
        device: &Device,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
    ) -> Result<vk::RenderPass> {
        let color_attch_desc = vk::AttachmentDescription::builder()
            .format(swapchain_props.format.format)
            .samples(msaa_samples)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let resolve_attch_desc = vk::AttachmentDescription::builder()
            .format(swapchain_props.format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let attch_descs = [color_attch_desc, resolve_attch_desc];

        let color_attch_ref = vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let resolve_attch_ref = vk::AttachmentReference::builder()
            .attachment(1)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let color_attchs = [color_attch_ref];

        let resolve_attchs = [resolve_attch_ref];

        let subpass_desc = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attchs)
            .resolve_attachments(&resolve_attchs)
            .build();

        let subpass_descs = [subpass_desc];

        let subpass_dep = vk::SubpassDependency::builder()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ
                    | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            )
            .build();

        let subpass_deps = [subpass_dep];

        let render_pass_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attch_descs)
            .subpasses(&subpass_descs)
            .dependencies(&subpass_deps)
            .build();

        let render_pass =
            unsafe { device.create_render_pass(&render_pass_info, None) }?;

        Ok(render_pass)
    }

    fn create_nodes(
        device: &Device,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
        id_format: vk::Format,
    ) -> Result<vk::RenderPass> {
        // attachments:
        // TODO depth

        let color_attch_desc = vk::AttachmentDescription::builder()
            .format(swapchain_props.format.format)
            .samples(msaa_samples)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let resolve_attch_desc = vk::AttachmentDescription::builder()
            .format(swapchain_props.format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let id_color_attch_desc = vk::AttachmentDescription::builder()
            .format(id_format)
            .samples(msaa_samples)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let id_resolve_attch_desc = vk::AttachmentDescription::builder()
            .format(id_format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let mask_attch_desc = vk::AttachmentDescription::builder()
            .format(vk::Format::R8G8B8A8_UNORM)
            .samples(msaa_samples)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::DONT_CARE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let mask_resolve_attch_desc = vk::AttachmentDescription::builder()
            .format(vk::Format::R8G8B8A8_UNORM)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .build();

        let attch_descs = [
            color_attch_desc,
            id_color_attch_desc,
            mask_attch_desc,
            resolve_attch_desc,
            id_resolve_attch_desc,
            mask_resolve_attch_desc,
        ];

        let color_attch_ref = vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let id_color_attch_ref = vk::AttachmentReference::builder()
            .attachment(1)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let mask_color_attch_ref = vk::AttachmentReference::builder()
            .attachment(2)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let resolve_attch_ref = vk::AttachmentReference::builder()
            .attachment(3)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let id_resolve_attch_ref = vk::AttachmentReference::builder()
            .attachment(4)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let mask_resolve_attch_ref = vk::AttachmentReference::builder()
            .attachment(5)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let color_attchs =
            [color_attch_ref, id_color_attch_ref, mask_color_attch_ref];
        let resolve_attchs = [
            resolve_attch_ref,
            id_resolve_attch_ref,
            mask_resolve_attch_ref,
        ];

        let subpass_desc = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attchs)
            .resolve_attachments(&resolve_attchs)
            .build();

        let subpass_descs = [subpass_desc];

        let subpass_dep = vk::SubpassDependency::builder()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ
                    | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            )
            .build();

        let subpass_deps = [subpass_dep];

        let render_pass_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attch_descs)
            .subpasses(&subpass_descs)
            .dependencies(&subpass_deps)
            .build();

        let render_pass =
            unsafe { device.create_render_pass(&render_pass_info, None) }?;

        Ok(render_pass)
    }

    fn create_selection_edge_detect(
        device: &Device,
        format: vk::Format,
    ) -> Result<vk::RenderPass> {
        let color_attch_desc = vk::AttachmentDescription::builder()
            .format(format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL)
            .build();

        let attch_descs = [color_attch_desc];

        let color_attch_ref = vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let color_attchs = [color_attch_ref];

        let subpass_desc = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attchs)
            .build();

        let subpass_descs = [subpass_desc];

        let subpass_dep = vk::SubpassDependency::builder()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ
                    | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            )
            .build();

        let subpass_deps = [subpass_dep];

        let render_pass_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attch_descs)
            .subpasses(&subpass_descs)
            .dependencies(&subpass_deps)
            .build();

        let render_pass =
            unsafe { device.create_render_pass(&render_pass_info, None) }?;

        Ok(render_pass)
    }

    fn create_selection_blur(
        device: &Device,
        swapchain_props: SwapchainProperties,
    ) -> Result<vk::RenderPass> {
        let color_attch_desc = vk::AttachmentDescription::builder()
            .format(swapchain_props.format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let attch_descs = [color_attch_desc];

        let color_attch_ref = vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let color_attchs = [color_attch_ref];

        let subpass_desc = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attchs)
            .build();

        let subpass_descs = [subpass_desc];

        let subpass_dep = vk::SubpassDependency::builder()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ
                    | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            )
            .build();

        let subpass_deps = [subpass_dep];

        let render_pass_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attch_descs)
            .subpasses(&subpass_descs)
            .dependencies(&subpass_deps)
            .build();

        let render_pass =
            unsafe { device.create_render_pass(&render_pass_info, None) }?;

        Ok(render_pass)
    }

    fn create_gui(
        device: &Device,
        swapchain_props: SwapchainProperties,
    ) -> Result<vk::RenderPass> {
        let color_attch_desc = vk::AttachmentDescription::builder()
            .format(swapchain_props.format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .build();

        let attch_descs = [color_attch_desc]; //, resolve_attch_desc];

        let color_attch_ref = vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let color_attchs = [color_attch_ref];

        let subpass_desc = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attchs)
            .build();

        let subpass_descs = [subpass_desc];

        let subpass_dep = vk::SubpassDependency::builder()
            .src_subpass(vk::SUBPASS_EXTERNAL)
            .dst_subpass(0)
            .src_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .src_access_mask(vk::AccessFlags::empty())
            .dst_stage_mask(vk::PipelineStageFlags::COLOR_ATTACHMENT_OUTPUT)
            .dst_access_mask(
                vk::AccessFlags::COLOR_ATTACHMENT_READ
                    | vk::AccessFlags::COLOR_ATTACHMENT_WRITE,
            )
            .build();

        let subpass_deps = [subpass_dep];

        let render_pass_info = vk::RenderPassCreateInfo::builder()
            .attachments(&attch_descs)
            .subpasses(&subpass_descs)
            .dependencies(&subpass_deps)
            .build();

        let render_pass =
            unsafe { device.create_render_pass(&render_pass_info, None) }?;

        Ok(render_pass)
    }
}

pub fn create_swapchain_framebuffers(
    device: &Device,
    image_views: &[vk::ImageView],
    color_texture: Texture,
    render_pass: vk::RenderPass,
    swapchain_props: SwapchainProperties,
) -> Vec<vk::Framebuffer> {
    image_views
        .iter()
        .map(|view| {
            let attachments = [color_texture.view, *view];

            let framebuffer_info = vk::FramebufferCreateInfo::builder()
                .render_pass(render_pass)
                .attachments(&attachments)
                .width(swapchain_props.extent.width)
                .height(swapchain_props.extent.height)
                .layers(1)
                .build();

            unsafe {
                device.create_framebuffer(&framebuffer_info, None).unwrap()
            }
        })
        .collect()
}
