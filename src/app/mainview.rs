use handlegraph::handle::NodeId;
use vulkano::device::{Device, DeviceExtensions, RawDeviceExtensions};
use vulkano::format::Format;
use vulkano::framebuffer::{Framebuffer, FramebufferAbstract, RenderPassAbstract, Subpass};
use vulkano::image::{ImageUsage, SwapchainImage};
use vulkano::instance::debug::{DebugCallback, MessageSeverity, MessageType};
use vulkano::instance::{Instance, PhysicalDevice};
use vulkano::{
    buffer::{BufferUsage, CpuAccessibleBuffer, CpuBufferPool, ImmutableBuffer},
    image::{AttachmentImage, Dimensions},
};
use vulkano::{
    command_buffer::{AutoCommandBufferBuilder, DynamicState, SubpassContents},
    pipeline::vertex::TwoBuffersDefinition,
};
use vulkano::{
    descriptor::{descriptor_set::PersistentDescriptorSet, PipelineLayoutAbstract},
    device::Queue,
};

use vulkano::pipeline::{viewport::Viewport, GraphicsPipeline};

use vulkano::swapchain::{
    self, AcquireError, ColorSpace, FullscreenExclusive, PresentMode, SurfaceTransform, Swapchain,
    SwapchainCreationError,
};

use crossbeam::channel;
use std::sync::Arc;
use std::time::Instant;
use vulkano::sync::{self, FlushError, GpuFuture};

use rgb::*;

use nalgebra_glm as glm;

use anyhow::{Context, Result};

use crate::geometry::*;
use crate::gfa::*;
use crate::input::*;
// use crate::layout::physics;
// use crate::layout::*;
use crate::render::*;
use crate::ui::{UICmd, UIState, UIThread};
use crate::view;
use crate::view::View;

pub struct MainView {
    node_draw_system: NodeDrawSystem,
    line_draw_system: LineDrawSystem,
    initial_view: View,
    view: View,
    vertices: Vec<Vertex>,
    draw_grid: bool,
    anim_handler: AnimHandler,
    // anim_thread: UIThread,
    // anim_cmd_tx: channel::Sender<UICmd>,
}

impl MainView {
    // pub fn new(gfx_queue: Arc<Queue>, subpass: Subpass<R>) -> NodeDr
    pub fn new<R>(gfx_queue: Arc<Queue>, render_pass: &Arc<R>) -> Result<MainView>
    where
        R: RenderPassAbstract + Send + Sync + 'static,
    {
        let node_draw_system = {
            // todo map Option -> Result
            let subpass = Subpass::from(render_pass.clone(), 0).unwrap();
            // Ok(NodeDrawSystem::new(gfx_queue.clone(), subpass))
            NodeDrawSystem::new(gfx_queue.clone(), subpass)
        };

        let line_draw_system = {
            // todo map Option -> Result
            let subpass = Subpass::from(render_pass.clone(), 0).unwrap();
            // Ok(LineDrawSystem::new(gfx_queue.clone(), subpass))
            LineDrawSystem::new(gfx_queue.clone(), subpass)
        };

        let vertices: Vec<Vertex> = Vec::new();

        let draw_grid = false;

        let view = View::default();
        let initial_view = view;

        let anim_handler = AnimHandler::default();

        unimplemented!();
    }
}

pub enum DisplayLayer {
    Grid,
    Graph,
}

pub enum MainViewInput {
    MousePos(Point),
    MousePrimaryButton { pressed: bool, point: Point },
    MouseSecondaryButton { pressed: bool, point: Point },
    MouseWheel { delta: f32 },
    // ArrowKeys { up: bool, right: bool, down: bool, left: bool },
    KeyUp { pressed: bool },
    KeyRight { pressed: bool },
    KeyDown { pressed: bool },
    KeyLeft { pressed: bool },
}

pub enum MainViewRecvMsg {
    ResetView,
    SetView {
        center: Option<Point>,
        scale: Option<f32>,
    },
    SetLayer {
        layer: DisplayLayer,
        on: bool,
    },
    ToggleLayer {
        layer: DisplayLayer,
    },
}

pub enum MainViewSendMsg {
    NodeAtScreenPoint {
        point: Point,
        node: NodeId,
    },
    // world coordinates
    ViewExtent {
        top_left: Point,
        bottom_right: Point,
    },
}

#[derive(Debug, Default)]
struct AnimHandler {
    mouse_pan_screen_origin: Option<Point>,
    view_anim_target: Option<View>,
    view_pan_const: Point,
    view_pan_delta: Point,
    view_scale_delta: f32,
    settings: AnimSettings,
    // view_pan_accel: Point,
    // view_scale_accel: f32,
}

impl AnimHandler {
    fn update(&mut self, mut view: View, mouse_pos: Option<Point>, dt: f32) -> View {
        view.scale += view.scale * dt * self.view_scale_delta;

        if let Some(min_scale) = self.settings.min_view_scale {
            view.scale = view.scale.max(min_scale);
        }

        if let Some(max_scale) = self.settings.max_view_scale {
            view.scale = view.scale.min(max_scale);
        }

        let dxy = match (self.mouse_pan_screen_origin, mouse_pos) {
            (Some(origin), Some(mouse_pos)) => (mouse_pos - origin) / 100.0,
            _ => (self.view_pan_const + self.view_pan_delta) * dt,
        };

        view.center += dxy * view.scale;

        let zoom_friction = 1.0 - (10.0_f32.powf(dt - 1.0));
        let pan_friction = 1.0 - (10.0_f32.powf(dt - 1.0));

        self.view_pan_delta *= pan_friction;
        self.view_scale_delta *= zoom_friction;

        if self.view_scale_delta.abs() < 0.00001 {
            self.view_scale_delta = 0.0;
        }

        view
    }

    fn start_mouse_pan(&mut self, origin: Point) {
        self.mouse_pan_screen_origin = Some(origin);
    }

    fn end_mouse_pan(&mut self) {
        self.mouse_pan_screen_origin = None;
    }

    /// If a direction is `None`, don't update the corresponding view delta const
    fn pan_const(&mut self, dx: Option<f32>, dy: Option<f32>) {
        // if a direction is set to zero, set the regular pan delta to
        // the old constant speed, and let the pan_friction in
        // update() smoothly bring it down
        if Some(0.0) == dx {
            self.view_pan_delta.x = self.view_pan_const.x;
        }

        if Some(0.0) == dy {
            self.view_pan_delta.y = self.view_pan_const.y;
        }

        let dxy = Point {
            x: dx.unwrap_or(self.view_pan_const.x),
            y: dy.unwrap_or(self.view_pan_const.y),
        };
        self.view_pan_const = dxy;
    }

    fn pan_delta(&mut self, dxy: Point) {
        self.view_pan_delta += dxy;

        if let Some(max_speed) = self.settings.max_speed {
            let d = &mut self.view_pan_delta;
            d.x = d.x.clamp(-max_speed, max_speed);
            d.y = d.y.clamp(-max_speed, max_speed);
        }
    }
}

#[derive(Debug)]
struct AnimSettings {
    // zoom_friction:
    // pan_friction:
    min_view_scale: Option<f32>,
    max_view_scale: Option<f32>,
    max_speed: Option<f32>,
}

impl std::default::Default for AnimSettings {
    fn default() -> Self {
        Self {
            min_view_scale: Some(0.5),
            max_view_scale: None,
            max_speed: Some(600.0),
        }
    }
}
