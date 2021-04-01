use ash::version::DeviceV1_0;
use ash::{vk, Device};

use super::texture::*;
use super::SwapchainProperties;

use anyhow::Result;

pub struct RenderPasses {
    nodes: vk::RenderPass,
    // offscreen: vk::RenderPass,
    // offscreen_msaa: vk::RenderPass,
    selection_edge_detect: vk::RenderPass,
    selection_blur: vk::RenderPass,
    gui: vk::RenderPass,
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

        let id_color_attch_desc = vk::AttachmentDescription::builder()
            .format(vk::Format::R32_UINT)
            .samples(vk::SampleCountFlags::TYPE_1)
            .load_op(vk::AttachmentLoadOp::CLEAR)
            .store_op(vk::AttachmentStoreOp::STORE)
            .initial_layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .final_layout(vk::ImageLayout::TRANSFER_SRC_OPTIMAL)
            .build();

        let attch_descs =
            [color_attch_desc, resolve_attch_desc, id_color_attch_desc];

        let color_attch_ref = vk::AttachmentReference::builder()
            .attachment(0)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let resolve_attch_ref = vk::AttachmentReference::builder()
            .attachment(1)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let id_color_attch_ref = vk::AttachmentReference::builder()
            .attachment(2)
            .layout(vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL)
            .build();

        let color_attchs = [color_attch_ref, id_color_attch_ref];
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
