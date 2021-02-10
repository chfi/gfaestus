use vulkano::command_buffer::{AutoCommandBufferBuilder, DynamicState, SubpassContents};
use vulkano::descriptor::{descriptor_set::PersistentDescriptorSet, PipelineLayoutAbstract};
use vulkano::device::{Device, DeviceExtensions};
use vulkano::framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass};
use vulkano::image::{ImageUsage, SwapchainImage};
use vulkano::instance::{Instance, PhysicalDevice};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer, CpuBufferPool},
    image::{AttachmentImage, Dimensions},
};

use vulkano::pipeline::{viewport::Viewport, GraphicsPipeline};

use vulkano::swapchain::{
    self, AcquireError, ColorSpace, FullscreenExclusive, PresentMode, SurfaceTransform, Swapchain,
    SwapchainCreationError,
};
use vulkano::sync::{self, FlushError, GpuFuture};

use vulkano_win::VkSurfaceBuild;

use winit::event::{Event, KeyboardInput, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

use std::sync::Arc;

use std::time::Instant;

use crossbeam::channel;

use crate::geometry::*;
use crate::gfa::*;
use crate::view;
use crate::view::View;

use super::{UICmd, UIState, UIThread};

pub fn keyboard_input(ui_cmd_tx: &channel::Sender<UICmd>, input: KeyboardInput) {
    use winit::event::VirtualKeyCode as Key;

    let state = input.state;
    let keycode = input.virtual_keycode;

    let pressed = state == winit::event::ElementState::Pressed;

    let speed = 200.0;

    if let Some(key) = keycode {
        match key {
            Key::Up => {
                if pressed {
                    let delta = Point { x: 0.0, y: speed };
                    ui_cmd_tx.send(UICmd::Pan { delta }).unwrap();
                }
            }
            Key::Right => {
                if pressed {
                    let delta = Point { x: -speed, y: 0.0 };
                    ui_cmd_tx.send(UICmd::Pan { delta }).unwrap();
                }
            }
            Key::Down => {
                if pressed {
                    let delta = Point { x: 0.0, y: -speed };
                    ui_cmd_tx.send(UICmd::Pan { delta }).unwrap();
                }
            }
            Key::Left => {
                if pressed {
                    let delta = Point { x: speed, y: 0.0 };
                    ui_cmd_tx.send(UICmd::Pan { delta }).unwrap();
                }
            }
            _ => {}
        }
    }
}

pub fn mouse_wheel_input(
    ui_cmd_tx: &channel::Sender<UICmd>,
    delta: winit::event::MouseScrollDelta,
) {
    use winit::event::MouseScrollDelta as ScrollDelta;
    match delta {
        ScrollDelta::LineDelta(_x, y) => {
            if y > 0.0 {
                ui_cmd_tx.send(UICmd::Zoom { delta: -0.45 }).unwrap();
            } else if y < 0.0 {
                ui_cmd_tx.send(UICmd::Zoom { delta: 0.45 }).unwrap();
            }
        }
        ScrollDelta::PixelDelta(_pos) => {}
    }
}
