use handlegraph::handle::NodeId;
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
    command_buffer::AutoCommandBuffer,
    device::{Device, DeviceExtensions, RawDeviceExtensions},
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
use crate::view;
use crate::view::View;

pub struct MainView {
    node_draw_system: NodeDrawSystem,
    line_draw_system: LineDrawSystem,
    pub view: View,
    vertices: Vec<Vertex>,
    draw_grid: bool,
    pub anim_handler: AnimHandler,
    base_node_width: f32,
    // anim_thread: UIThread,
    // anim_cmd_tx: channel::Sender<UICmd>,
}

impl MainView {
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

        let anim_handler = AnimHandler::new(view);

        let base_node_width = 100.0;

        Ok(Self {
            node_draw_system,
            line_draw_system,
            vertices,
            draw_grid,
            view,
            anim_handler,
            base_node_width,
        })
    }

    pub fn view(&self) -> View {
        self.view
    }

    pub fn tick_animation(&mut self, mouse_pos: Option<Point>, dt: f32) {
        let new_view = self.anim_handler.update(self.view, mouse_pos, dt);
        self.view = new_view;
    }

    pub fn set_initial_view(&mut self, center: Option<Point>, scale: Option<f32>) {
        let center = center.unwrap_or(self.anim_handler.initial_view.center);
        let scale = scale.unwrap_or(self.anim_handler.initial_view.scale);
        self.anim_handler.initial_view = View { center, scale };
    }

    pub fn reset_view(&mut self) {
        self.view = self.anim_handler.initial_view;
    }

    pub fn set_vertices<VI>(&mut self, vertices: VI)
    where
        VI: IntoIterator<Item = Vertex>,
        VI::IntoIter: ExactSizeIterator,
    {
        self.vertices.clear();
        self.vertices.extend(vertices.into_iter());
    }

    pub fn has_vertices(&self) -> bool {
        !self.vertices.is_empty()
    }

    pub fn draw_nodes(
        &mut self,
        dynamic_state: &DynamicState,
        offset: Point,
    ) -> Result<AutoCommandBuffer> {
        let node_width = {
            let mut width = self.base_node_width;
            if self.view.scale > 100.0 {
                width *= self.view.scale / 100.0;
            }
            width
        };
        self.node_draw_system.draw(
            dynamic_state,
            self.vertices.iter().copied(),
            self.view,
            offset,
            node_width,
        )
    }

    pub fn draw_nodes_dynamic<VI>(
        &mut self,
        dynamic_state: &DynamicState,
        vertices: VI,
        offset: Point,
    ) -> Result<AutoCommandBuffer>
    where
        VI: IntoIterator<Item = Vertex>,
        VI::IntoIter: ExactSizeIterator,
    {
        let node_width = {
            let mut width = self.base_node_width;
            if self.view.scale > 100.0 {
                width *= self.view.scale / 100.0;
            }
            width
        };
        self.node_draw_system
            .draw(dynamic_state, vertices, self.view, offset, node_width)
    }

    pub fn read_node_id_at(
        &self,
        screen_width: u32,
        screen_height: u32,
        point: Point,
    ) -> Option<u32> {
        self.node_draw_system
            .read_node_id_at(screen_width, screen_height, point)
    }

    pub fn add_lines(
        &mut self,
        lines: &[(Point, Point)],
        color: RGB<f32>,
    ) -> Result<(usize, Box<dyn GpuFuture>)> {
        self.line_draw_system.add_lines(lines, color)
    }

    pub fn draw_lines(&self, dynamic_state: &DynamicState) -> Result<AutoCommandBuffer> {
        self.line_draw_system.draw_stored(dynamic_state, self.view)
    }

    pub fn set_view_center(&mut self, center: Point) {
        self.view.center = center;
    }

    pub fn set_view_scale(&mut self, scale: f32) {
        self.view.scale = scale;
    }

    pub fn set_mouse_pan(&mut self, origin: Option<Point>) {
        match origin {
            Some(origin) => self.anim_handler.start_mouse_pan(origin),
            None => self.anim_handler.end_mouse_pan(),
        }
    }

    pub fn pan_const(&mut self, dx: Option<f32>, dy: Option<f32>) {
        self.anim_handler.pan_const(dx, dy);
    }

    pub fn pan_delta(&mut self, dxy: Point) {
        self.anim_handler.pan_delta(dxy);
    }

    pub fn zoom_delta(&mut self, dz: f32) {
        self.anim_handler.zoom_delta(self.view.scale, dz)
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
pub struct AnimHandler {
    mouse_pan_screen_origin: Option<Point>,
    view_anim_target: Option<View>,
    view_pan_const: Point,
    view_pan_delta: Point,
    view_scale_delta: f32,
    settings: AnimSettings,
    initial_view: View,
    // view_pan_accel: Point,
    // view_scale_accel: f32,
}

impl AnimHandler {
    fn new(initial_view: View) -> Self {
        Self {
            initial_view,
            ..AnimHandler::default()
        }
    }

    fn update(&mut self, mut view: View, mouse_pos: Option<Point>, dt: f32) -> View {
        view.scale += view.scale * dt * self.view_scale_delta;

        if let Some(min_scale) = self.settings.min_view_scale {
            view.scale = view.scale.max(min_scale);
        }

        if let Some(max_scale) = self.settings.max_view_scale {
            view.scale = view.scale.min(max_scale);
        }

        let dxy = match (self.mouse_pan_screen_origin, mouse_pos) {
            (Some(origin), Some(mouse_pos)) => {
                (mouse_pos - origin) * self.settings.mouse_pan_mult * dt
            }
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

    fn zoom_delta(&mut self, current_scale: f32, dz: f32) {
        let delta_mult = current_scale.log2();
        let delta_mult = delta_mult.max(1.0);
        self.view_scale_delta += dz * delta_mult;
    }
}

#[derive(Debug)]
struct AnimSettings {
    // zoom_friction:
    // pan_friction:
    min_view_scale: Option<f32>,
    max_view_scale: Option<f32>,
    max_speed: Option<f32>,
    mouse_pan_mult: f32,
}

impl std::default::Default for AnimSettings {
    fn default() -> Self {
        Self {
            min_view_scale: Some(0.5),
            max_view_scale: None,
            max_speed: Some(600.0),
            mouse_pan_mult: 1.0,
        }
    }
}
