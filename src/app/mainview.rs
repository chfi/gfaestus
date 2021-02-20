use vulkano::command_buffer::AutoCommandBuffer;
use vulkano::command_buffer::DynamicState;
use vulkano::device::Queue;
use vulkano::framebuffer::{RenderPassAbstract, Subpass};
use vulkano::sync::GpuFuture;

use crossbeam::atomic::AtomicCell;
use crossbeam::channel;

use std::sync::Arc;

use rgb::*;

use nalgebra_glm as glm;

use anyhow::{Context, Result};

use handlegraph::handle::NodeId;

use crate::geometry::*;
use crate::gfa::*;
use crate::input::*;
use crate::render::*;
use crate::view::{ScreenDims, View};

pub struct MainView {
    node_draw_system: NodeDrawSystem,
    line_draw_system: LineDrawSystem,
    view: Arc<AtomicCell<View>>,
    vertices: Vec<Vertex>,
    draw_grid: bool,
    pub anim_handler: AnimHandler,
    base_node_width: f32,
}

impl MainView {
    pub fn anim_handler_thread(&self) -> AnimHandlerThread {
        anim_handler_thread(self.anim_handler.clone(), self.view.clone())
    }

    pub fn new<R>(
        gfx_queue: Arc<Queue>,
        render_pass: &Arc<R>,
    ) -> Result<MainView>
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
        let view = Arc::new(AtomicCell::new(view));

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
        self.view.load()
    }

    pub fn tick_animation(&mut self, mouse_pos: Option<Point>, dt: f32) {
        self.anim_handler.update_cell(&self.view, mouse_pos, dt);
    }

    pub fn set_initial_view(
        &mut self,
        center: Option<Point>,
        scale: Option<f32>,
    ) {
        let center = center.unwrap_or(self.anim_handler.initial_view.center);
        let scale = scale.unwrap_or(self.anim_handler.initial_view.scale);
        self.anim_handler.initial_view = View { center, scale };
    }

    pub fn reset_view(&mut self) {
        self.view.store(self.anim_handler.initial_view);
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
        let view = self.view.load();
        let node_width = {
            let mut width = self.base_node_width;
            if view.scale > 100.0 {
                width *= view.scale / 100.0;
            }
            width
        };
        self.node_draw_system.draw(
            dynamic_state,
            self.vertices.iter().copied(),
            view,
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
        let view = self.view.load();
        let node_width = {
            let mut width = self.base_node_width;
            if view.scale > 100.0 {
                width *= view.scale / 100.0;
            }
            width
        };
        self.node_draw_system.draw(
            dynamic_state,
            vertices,
            view,
            offset,
            node_width,
        )
    }

    pub fn read_node_id_at<Dims: Into<ScreenDims>>(
        &self,
        screen_dims: Dims,
        point: Point,
    ) -> Option<u32> {
        self.node_draw_system.read_node_id_at(screen_dims, point)
    }

    pub fn add_lines(
        &mut self,
        lines: &[(Point, Point)],
        color: RGB<f32>,
    ) -> Result<(usize, Box<dyn GpuFuture>)> {
        self.line_draw_system.add_lines(lines, color)
    }

    pub fn draw_lines(
        &self,
        dynamic_state: &DynamicState,
    ) -> Result<AutoCommandBuffer> {
        let view = self.view.load();
        self.line_draw_system.draw_stored(dynamic_state, view)
    }

    pub fn set_view_center(&self, center: Point) {
        let mut view = self.view.load();
        view.center = center;
        self.view.store(view);
    }

    pub fn set_view_scale(&mut self, scale: f32) {
        let mut view = self.view.load();
        view.scale = scale;
        self.view.store(view);
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
        let view = self.view.load();
        self.anim_handler.zoom_delta(view.scale, dz)
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

#[derive(Debug, Clone, Copy, PartialEq)]
enum AnimMsg {
    SetMousePan(Option<Point>),
    PanConst { dx: Option<f32>, dy: Option<f32> },
    PanDelta { dxy: Point },
    ZoomDelta { dz: f32 },
}

pub struct AnimHandlerThread {
    view: Arc<AtomicCell<View>>,
    mouse_pos: Arc<AtomicCell<Option<Point>>>,
    _join_handle: std::thread::JoinHandle<()>,
    msg_tx: channel::Sender<AnimMsg>,
}

impl AnimHandlerThread {
    pub fn view(&self) -> View {
        self.view.load()
    }

    pub fn set_mouse_pos(&self, pos: Option<Point>) {
        self.mouse_pos.store(pos);
    }

    pub fn set_mouse_pan(&self, origin: Option<Point>) {
        self.msg_tx.send(AnimMsg::SetMousePan(origin)).unwrap();
    }

    pub fn pan_const(&self, dx: Option<f32>, dy: Option<f32>) {
        self.msg_tx.send(AnimMsg::PanConst { dx, dy }).unwrap();
    }

    pub fn pan_delta(&self, dxy: Point) {
        self.msg_tx.send(AnimMsg::PanDelta { dxy }).unwrap();
    }

    pub fn zoom_delta(&self, dz: f32) {
        self.msg_tx.send(AnimMsg::ZoomDelta { dz }).unwrap();
    }
}

pub fn anim_handler_thread(
    anim_handler: AnimHandler,
    view: Arc<AtomicCell<View>>,
) -> AnimHandlerThread {
    let mouse_pos = Arc::new(AtomicCell::new(None));

    let inner_view = view.clone();
    let inner_mouse_pos = mouse_pos.clone();

    let (msg_tx, msg_rx) = channel::unbounded::<AnimMsg>();

    let _join_handle = std::thread::spawn(move || {
        let update_delay = std::time::Duration::from_millis(5);
        let sleep_delay = std::time::Duration::from_micros(2500);

        let view = inner_view;
        let mouse_pos = inner_mouse_pos;
        let mut anim = anim_handler;

        let mut last_update = std::time::Instant::now();

        loop {
            while let Ok(msg) = msg_rx.try_recv() {
                match msg {
                    AnimMsg::SetMousePan(origin) => match origin {
                        Some(origin) => anim.start_mouse_pan(origin),
                        None => anim.end_mouse_pan(),
                    },
                    AnimMsg::PanConst { dx, dy } => {
                        anim.pan_const(dx, dy);
                    }
                    AnimMsg::PanDelta { dxy } => {
                        anim.pan_delta(dxy);
                    }
                    AnimMsg::ZoomDelta { dz } => {
                        let view = view.load();
                        anim.zoom_delta(view.scale, dz)
                    }
                }
            }

            if last_update.elapsed() > update_delay {
                let dt = last_update.elapsed().as_secs_f32();
                let pos = mouse_pos.load();
                anim.update_cell(&view, pos, dt);
                last_update = std::time::Instant::now();
            } else {
                std::thread::sleep(sleep_delay);
            }
        }
    });

    AnimHandlerThread {
        view,
        _join_handle,
        mouse_pos,
        msg_tx,
    }
}

#[derive(Debug, Default, Clone)]
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

    fn update_cell(
        &mut self,
        view: &AtomicCell<View>,
        mouse_pos: Option<Point>,
        dt: f32,
    ) {
        let before = view.load();
        let new = self.update(before, mouse_pos, dt);
        view.store(new);
    }

    fn update(
        &mut self,
        mut view: View,
        mouse_pos: Option<Point>,
        dt: f32,
    ) -> View {
        // println!("dt {}", dt);
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

        // let zoom_friction = 1.0 - (10.0_f32.powf(dt - 1.0));
        // let pan_friction = 1.0 - (10.0_f32.powf(dt - 1.0));

        let zoom_friction = if dt >= 1.0 {
            0.0
        } else {
            1.0 - (10.0_f32.powf(dt - 1.0))
        };
        let pan_friction = if dt >= 1.0 {
            0.0
        } else {
            1.0 - (10.0_f32.powf(dt - 1.0))
        };

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

#[derive(Debug, Clone, Copy)]
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
