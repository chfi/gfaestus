use ash::version::DeviceV1_0;
use ash::{vk, Device};

use super::texture::*;
use super::SwapchainProperties;

use anyhow::Result;

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
        .stencil_load_op(vk::AttachmentLoadOp::DONT_CARE)
        .stencil_store_op(vk::AttachmentStoreOp::DONT_CARE)
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

    let subpass_desc = vk::SubpassDescription::builder()
        .pipeline_bind_point(vk::PipelineBindPoint::GRAPHICS)
        .color_attachments(&[color_attch_ref])
        .resolve_attachments(&[resolve_attch_ref])
        .build();

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

    let render_pass_info = vk::RenderPassCreateInfo::builder()
        .attachments(&attch_descs)
        .subpasses(&[subpass_desc])
        .dependencies(&[subpass_dep])
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
