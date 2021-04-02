use ash::version::DeviceV1_0;
use ash::{vk, Device};

use super::context::VkContext;
use super::texture::*;
use super::GfaestusVk;
use super::SwapchainProperties;

use anyhow::Result;

pub struct RenderPasses {
    pub nodes: vk::RenderPass,
    // offscreen: vk::RenderPass,
    // offscreen_msaa: vk::RenderPass,
    pub selection_edge_detect: vk::RenderPass,
    pub selection_blur: vk::RenderPass,
    pub gui: vk::RenderPass,
}

pub struct Framebuffers {
    pub nodes: vk::Framebuffer,
    pub selection_edge_detect: vk::Framebuffer,
    pub selection_blur: vk::Framebuffer,
    pub gui: vk::Framebuffer,
}

impl Framebuffers {
    pub fn destroy(&self, device: &Device) {
        unsafe {
            device.destroy_framebuffer(self.nodes, None);
            device.destroy_framebuffer(self.selection_edge_detect, None);
            device.destroy_framebuffer(self.selection_blur, None);
            device.destroy_framebuffer(self.gui, None);
        }
    }
}

pub struct NodeAttachments {
    pub color: Texture,
    pub resolve: Texture,
    pub mask: Texture,
    pub id_color: Texture,
    pub id_resolve: Texture,
}

}

pub struct OffscreenAttachment {
    pub color: Texture,
}

impl OffscreenAttachment {
    pub fn new(
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        // app: &GfaestusVk,
        swapchain_props: SwapchainProperties,
        // format: vk::Format,
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
        // format: vk::Format,
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
                // .max_anisotropy(16.0)
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
            vk::ImageUsageFlags::COLOR_ATTACHMENT,
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
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
        // app: &GfaestusVk,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
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

        let mask =
            Self::mask(vk_context, command_pool, queue, swapchain_props)?;

        let id_color = Self::id_color(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
            msaa_samples,
        )?;

        let id_resolve =
            Self::id_resolve(vk_context, command_pool, queue, swapchain_props)?;

        Ok(Self {
            color,
            resolve,
            mask,
            id_resolve,
            id_color,
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
        self.mask =
            Self::mask(vk_context, command_pool, queue, swapchain_props)?;
        self.id_color = Self::id_color(
            vk_context,
            command_pool,
            queue,
            swapchain_props,
            msaa_samples,
        )?;
        self.id_resolve =
            Self::id_resolve(vk_context, command_pool, queue, swapchain_props)?;

        Ok(())
    }

    pub fn destroy(&mut self, device: &Device) {
        self.color.destroy(device);
        self.resolve.destroy(device);
        self.mask.destroy(device);
        self.id_color.destroy(device);
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

    fn id_resolve(
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        swapchain_props: SwapchainProperties,
    ) -> Result<Texture> {
        let extent = swapchain_props.extent;

        let color = Texture::create_attachment_image(
            vk_context,
            command_pool,
            queue,
            vk::ImageUsageFlags::COLOR_ATTACHMENT
                | vk::ImageUsageFlags::TRANSFER_SRC,
            // vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            // vk::ImageLayout::GENERAL,
            vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            extent,
            vk::Format::R32_UINT,
            None,
        )?;

        Ok(color)
    }

    fn mask(
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        // app: &GfaestusVk,
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
                // .max_anisotropy(16.0)
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
            vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            extent,
            vk::Format::R8G8B8A8_UNORM,
            Some(mask_sampler),
        )?;

        Ok(mask)
    }

    fn id_color(
        // app: &GfaestusVk,
        vk_context: &VkContext,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
    ) -> Result<Texture> {
        let extent = swapchain_props.extent;

        let format = vk::Format::R32_UINT;

        // let color = Texture::create_transient_color(
        //     vk_context,
        //     command_pool,
        //     queue,
        //     swapchain_props,
        //     msaa_samples,
        // )?;

        // Ok(color)

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
            format,
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
}

impl RenderPasses {
    pub fn create(
        device: &Device,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
    ) -> Result<Self> {
        let nodes = Self::create_nodes(device, swapchain_props, msaa_samples)?;
        let selection_edge_detect = Self::create_selection_edge_detect(
            device,
            swapchain_props,
            vk::Format::R8G8B8A8_UNORM,
        )?;
        let selection_blur =
            Self::create_selection_blur(device, swapchain_props)?;
        let gui = Self::create_gui(device, swapchain_props, msaa_samples)?;

        Ok(Self {
            nodes,
            selection_edge_detect,
            selection_blur,
            gui,
        })
    }

    pub fn framebuffers(
        &self,
        device: &Device,
        node_attachments: &NodeAttachments,
        offscreen_attachment: &OffscreenAttachment,
        gui_intermediary: Texture,
        swapchain_image_view: vk::ImageView,
        swapchain_props: SwapchainProperties,
    ) -> Result<Framebuffers> {
        let extent = swapchain_props.extent;

        let nodes = {
            let attachments = [
                node_attachments.color.view,
                node_attachments.id_color.view,
                swapchain_image_view,
                // node_attachments.resolve.view,
                node_attachments.id_resolve.view,
                // node_attachments.resolve.view,
            ];

            let framebuffer_info = vk::FramebufferCreateInfo::builder()
                .render_pass(self.nodes)
                .attachments(&attachments)
                .width(extent.width)
                .height(extent.height)
                .layers(1)
                .build();

            println!("node framebuffer {:#?}", framebuffer_info);

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
            // let attachments = [gui_intermediary.view, swapchain_image_view];
            let attachments = [swapchain_image_view];
            // let attachments =
            //     [node_attachments.resolve.view, swapchain_image_view];

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
            selection_edge_detect,
            selection_blur,
            gui,
        })
    }

    pub fn recreate(
        &mut self,
        device: &Device,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
    ) -> Result<()> {
        self.destroy(device);

        let nodes = Self::create_nodes(device, swapchain_props, msaa_samples)?;

        let selection_edge_detect = Self::create_selection_edge_detect(
            device,
            swapchain_props,
            vk::Format::R8G8B8A8_UNORM,
        )?;
        let selection_blur =
            Self::create_selection_blur(device, swapchain_props)?;
        let gui = Self::create_gui(device, swapchain_props, msaa_samples)?;

        self.nodes = nodes;
        self.selection_edge_detect = selection_edge_detect;
        self.selection_blur = selection_blur;
        self.gui = gui;

        Ok(())
    }

    pub fn destroy(&self, device: &Device) {
        unsafe {
            device.destroy_render_pass(self.nodes, None);
            device.destroy_render_pass(self.selection_edge_detect, None);
            device.destroy_render_pass(self.selection_blur, None);
            device.destroy_render_pass(self.gui, None);
        }
    }

    fn create_nodes(
        device: &Device,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
    ) -> Result<vk::RenderPass> {
        // attachments:
        // color + resolve
        // TODO ID
        // TODO depth
        // TODO mask + resolve

        let color_attch_desc = vk::AttachmentDescription::builder()
            .format(swapchain_props.format.format)
            .samples(msaa_samples)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let resolve_attch_desc = vk::AttachmentDescription::builder()
            .format(swapchain_props.format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::UNDEFINED)
            // .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            // .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .build();

        let id_color_attch_desc = vk::AttachmentDescription::builder()
            .format(vk::Format::R32_UINT)
            .samples(msaa_samples)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            // .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .build();

        let id_resolve_attch_desc = vk::AttachmentDescription::builder()
            .format(vk::Format::R32_UINT)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::DONT_CARE)
            .store_op(vk::AttachmentStoreOp::STORE)
            // .initial_layout(vk::ImageLayout::GENERAL)
            // .final_layout(vk::ImageLayout::GENERAL)
            .initial_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            // .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .build();

        let attch_descs = [
            color_attch_desc,
            id_color_attch_desc,
            resolve_attch_desc,
            id_resolve_attch_desc,
        ];

        let color_attch_ref = vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let id_color_attch_ref = vk::AttachmentReference::builder()
            .attachment(1)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let resolve_attch_ref = vk::AttachmentReference::builder()
            .attachment(2)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let id_resolve_attch_ref = vk::AttachmentReference::builder()
            .attachment(3)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let color_attchs = [color_attch_ref, id_color_attch_ref];
        let resolve_attchs = [resolve_attch_ref, id_resolve_attch_ref];

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

        println!("render_pass_info {:#?}", render_pass_info);

        let render_pass =
            unsafe { device.create_render_pass(&render_pass_info, None) }?;

        Ok(render_pass)
    }

    fn create_selection_edge_detect(
        device: &Device,
        swapchain_props: SwapchainProperties,
        format: vk::Format,
    ) -> Result<vk::RenderPass> {
        let color_attch_desc = vk::AttachmentDescription::builder()
            .format(format)
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

    fn create_selection_blur(
        device: &Device,
        swapchain_props: SwapchainProperties,
    ) -> Result<vk::RenderPass> {
        let color_attch_desc = vk::AttachmentDescription::builder()
            .format(swapchain_props.format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::LOAD)
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
        let resolve_attchs = [];

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

    fn create_gui(
        device: &Device,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
    ) -> Result<vk::RenderPass> {
        let color_attch_desc = vk::AttachmentDescription::builder()
            .format(swapchain_props.format.format)
            // .samples(msaa_samples)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            // .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .build();

        /*
        let resolve_attch_desc = vk::AttachmentDescription::builder()
            .format(swapchain_props.format.format)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::LOAD)
            .store_op(vk::AttachmentStoreOp::STORE)
            // .initial_layout(vk::ImageLayout::UNDEFINED)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
            .build();
        */

        let attch_descs = [color_attch_desc]; //, resolve_attch_desc];

        let color_attch_ref = vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        // let resolve_attch_ref = vk::AttachmentReference::builder()
        //     .attachment(1)
        //     .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        //     .build();

        let color_attchs = [color_attch_ref];
        // let resolve_attchs = [resolve_attch_ref];
        // let resolve_attchs = [];

        let subpass_desc = vk::SubpassDescription::builder()
            .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
            .color_attachments(&color_attchs)
            // .resolve_attachments(&resolve_attchs)
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

pub fn create_swapchain_render_pass_dont_clear(
    device: &Device,
    swapchain_props: SwapchainProperties,
    msaa_samples: vk::SampleCountFlags,
) -> Result<vk::RenderPass> {
    let color_attch_desc = vk::AttachmentDescription::builder()
        .format(swapchain_props.format.format)
        .samples(msaa_samples)
        .load_op(vk::AttachmentLoadOp::LOAD)
        .store_op(vk::AttachmentStoreOp::STORE)
        .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .build();

    let resolve_attch_desc = vk::AttachmentDescription::builder()
        .format(swapchain_props.format.format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::DONT_CARE)
        .store_op(vk::AttachmentStoreOp::STORE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
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

pub fn create_swapchain_render_pass(
    device: &Device,
    swapchain_props: SwapchainProperties,
    msaa_samples: vk::SampleCountFlags,
) -> Result<vk::RenderPass> {
    let color_attch_desc = vk::AttachmentDescription::builder()
        .format(swapchain_props.format.format)
        .samples(msaa_samples)
        .load_op(vk::AttachmentLoadOp::CLEAR)
        .store_op(vk::AttachmentStoreOp::STORE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
        .build();

    let resolve_attch_desc = vk::AttachmentDescription::builder()
        .format(swapchain_props.format.format)
        .samples(vk::SampleCountFlags::TYPE_1)
        .load_op(vk::AttachmentLoadOp::DONT_CARE)
        .store_op(vk::AttachmentStoreOp::STORE)
        .initial_layout(vk::ImageLayout::UNDEFINED)
        .final_layout(vk::ImageLayout::PRESENT_SRC_KHR)
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
