use vulkano::command_buffer::{AutoCommandBuffer, AutoCommandBufferBuilder};
use vulkano::device::Queue;
use vulkano::framebuffer::{RenderPassAbstract, Subpass};
use vulkano::sync::GpuFuture;
use vulkano::{command_buffer::DynamicState, sampler::Sampler};

use crossbeam::atomic::AtomicCell;
use crossbeam::channel;

use std::sync::Arc;

use rgb::*;

use anyhow::Result;

use handlegraph::handle::NodeId;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::geometry::*;
use crate::render::nodes::OverlayCache;
use crate::render::*;
use crate::view::{ScreenDims, View};
use vulkano::command_buffer::AutoCommandBufferBuilderContextError;

// use crate::input::binds::*;
use crate::input::binds::{
    BindableInput, InputPayload, KeyBind, MouseButtonBind, SystemInput,
    SystemInputBindings, WheelBind,
};
use crate::input::MousePos;

use super::{
    node_flags::{FlagUpdate, LayoutFlags, NodeFlag, NodeFlags},
    theme::{Theme, ThemeId},
};

use crate::vulkan::{
    context::VkContext,
    draw_system::{
        nodes::{NodePipelines, NodeThemePipeline, NodeVertices},
        NodeDrawAsh,
    },
    GfaestusVk, SwapchainProperties,
};

use ash::vk;

pub struct MainView {
    // pub node_draw_system: crate::vulkan::draw_system::NodeDrawAsh,
    pub node_draw_system: crate::vulkan::draw_system::nodes::NodePipelines,

    base_node_width: f32,

    view: Arc<AtomicCell<View>>,
    anim_handler_thread: AnimHandlerThread,
}

#[derive(Debug, Default, Clone)]
pub struct NodeData {
    vertices: Vec<Vertex>,
    flags: LayoutFlags,
    // _updates: FxHashSet<NodeId>
}

impl MainView {
    pub fn new(
        app: &GfaestusVk,
        // vk_context: &VkContext,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
    ) -> Result<Self> {
        // let node_draw_system = NodeDrawAsh::new(
        //     vk_context,
        //     swapchain_props,
        //     msaa_samples,
        //     render_pass,
        // )?;

        let node_draw_system = NodePipelines::new(
            app,
            swapchain_props,
            msaa_samples,
            render_pass,
        )?;

        let base_node_width = 100.0;

        let view = View::default();

        let anim_handler = AnimHandler::default();
        let view = Arc::new(AtomicCell::new(view));

        let screen_dims = {
            let extent = swapchain_props.extent;
            ScreenDims {
                width: extent.width as f32,
                height: extent.height as f32,
            }
        };

        let anim_handler_thread =
            anim_handler_thread(anim_handler, screen_dims, view.clone());

        let main_view = Self {
            node_draw_system,
            base_node_width,

            view,
            anim_handler_thread,
        };

        Ok(main_view)
    }

    pub fn view(&self) -> View {
        self.view.load()
    }

    pub fn set_initial_view(&self, center: Option<Point>, scale: Option<f32>) {
        let old_init_view = self.anim_handler_thread.initial_view.load();
        let center = center.unwrap_or(old_init_view.center);
        let scale = scale.unwrap_or(old_init_view.scale);
        self.anim_handler_thread
            .initial_view
            .store(View { center, scale });
    }

    pub fn reset_view(&self) {
        self.view
            .store(self.anim_handler_thread.initial_view.load());
    }

    pub fn draw_nodes(
        &self,
        cmd_buf: vk::CommandBuffer,
        render_pass: vk::RenderPass,
        framebuffer: vk::Framebuffer,
        framebuffer_dc: vk::Framebuffer,
        screen_dims: [f32; 2],
        offset: Point,
    ) -> Result<()> {
        let view = self.view.load();

        self.node_draw_system.draw_themed(
            cmd_buf,
            render_pass,
            framebuffer,
            framebuffer_dc,
            screen_dims,
            self.base_node_width,
            view,
            offset,
            0,
        )
    }

    pub fn set_view_center(&self, center: Point) {
        let mut view = self.view.load();
        view.center = center;
        self.view.store(view);
    }

    pub fn set_view_scale(&self, scale: f32) {
        let mut view = self.view.load();
        view.scale = scale;
        self.view.store(view);
    }

    pub fn set_screen_dims<D: Into<ScreenDims>>(&self, dims: D) {
        self.anim_handler_thread.screen_dims.store(dims.into());
    }

    pub fn set_mouse_pos(&self, pos: Option<Point>) {
        self.anim_handler_thread.mouse_pos.store(pos);
    }

    pub fn set_mouse_pan(&self, origin: Option<Point>) {
        self.anim_handler_thread.set_mouse_pan(origin);
    }

    pub fn pan_const(&self, dx: Option<f32>, dy: Option<f32>) {
        self.anim_handler_thread.pan_const(dx, dy);
    }

    pub fn pan_delta(&self, dxy: Point) {
        self.anim_handler_thread.pan_delta(dxy);
    }

    pub fn zoom_delta(&self, dz: f32) {
        self.anim_handler_thread.zoom_delta(dz)
    }

    pub fn apply_input<Dims: Into<ScreenDims>>(
        &self,
        screen_dims: Dims,
        app_msg_tx: &channel::Sender<crate::app::AppMsg>,
        input: SystemInput<MainViewInput>,
    ) {
        use MainViewInput as In;
        let payload = input.payload();

        match input {
            SystemInput::Keyboard { state, .. } => {
                let pressed = state.pressed();

                let pan_delta = |invert: bool| {
                    let delta = if pressed { 1.0 } else { 0.0 };
                    if invert {
                        -delta
                    } else {
                        delta
                    }
                };

                match payload {
                    In::KeyPanUp => {
                        self.pan_const(None, Some(pan_delta(true)));
                    }
                    In::KeyPanRight => {
                        self.pan_const(Some(pan_delta(false)), None);
                    }
                    In::KeyPanDown => {
                        self.pan_const(None, Some(pan_delta(false)));
                    }
                    In::KeyPanLeft => {
                        self.pan_const(Some(pan_delta(true)), None);
                    }
                    In::KeyResetView => {
                        if pressed {
                            self.reset_view();
                        }
                    }
                    _ => (),
                }
            }
            SystemInput::MouseButton { pos, state, .. } => {
                let pressed = state.pressed();
                match payload {
                    In::ButtonMousePan => {
                        if pressed {
                            self.set_mouse_pan(Some(pos));
                        } else {
                            self.set_mouse_pan(None);
                        }
                    }
                    In::ButtonSelect => {
                        use crate::app::AppMsg;
                        use crate::app::Select;

                        /*
                        let selected_node = self
                            .read_node_id_at(screen_dims, pos)
                            .map(|nid| NodeId::from(nid as u64));

                        if let Some(node) = selected_node {
                            app_msg_tx
                                .send(AppMsg::Selection(Select::One {
                                    node,
                                    clear: false,
                                }))
                                .unwrap();
                        }
                        */
                    }
                    _ => (),
                }
            }
            SystemInput::Wheel { delta, .. } => {
                if let In::WheelZoom = payload {
                    self.zoom_delta(delta);
                }
            }
        }
    }
}

// impl MainView {
/*
    pub fn prepare_themes(
        &self,
        sampler: &Arc<Sampler>,
        primary: &Theme,
        secondary: &Theme,
    ) -> Result<()> {
        self.node_draw_system
            .prepare_themes(sampler, primary, secondary)
    }

    pub fn cache_theme(
        &self,
        sampler: &Arc<Sampler>,
        theme_id: ThemeId,
        theme: &Theme,
    ) -> Result<()> {
        self.node_draw_system.set_theme(sampler, theme_id, theme)
    }

    pub fn valid_theme_cache(&self, theme_id: ThemeId, hash: u64) -> bool {
        if let Some(cached) = self.node_draw_system.cached_theme_hash(theme_id)
        {
            cached == hash
        } else {
            false
        }
    }

    pub fn build_overlay_cache<I>(
        &self,
        colors: I,
    ) -> Result<(OverlayCache, Box<dyn GpuFuture>)>
    where
        I: Iterator<Item = rgb::RGB<f32>>,
    {
        self.node_draw_system.build_overlay_cache(colors)
    }

    pub fn view(&self) -> View {
        self.view.load()
    }


    pub fn update_node_selection(
        &mut self,
        new_selection: &FxHashSet<NodeId>,
    ) -> Result<()> {
        let draw_sys = &self.node_draw_system;
        let flags = &mut self.node_data.flags;

        draw_sys.update_node_selection(|buffer| {
            let _result = flags.update_selection(new_selection, buffer)?;
            Ok(())
        })
    }

    pub fn clear_node_selection(&mut self) -> Result<()> {
        let draw_sys = &self.node_draw_system;
        let flags = &mut self.node_data.flags;

        flags.clear();

        draw_sys.update_node_selection(|buffer| {
            let _result = flags.clear_buffer(buffer)?;
            Ok(())
        })?;

        Ok(())
    }

    pub fn has_vertices(&self) -> bool {
        !self.node_data.vertices.is_empty()
    }


    pub fn read_node_id_at<Dims: Into<ScreenDims>>(
        &self,
        screen_dims: Dims,
        point: Point,
    ) -> Option<u32> {
        self.node_draw_system.read_node_id_at(screen_dims, point)
    }

    pub fn add_lines(
        &self,
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

*/

// }

pub enum DisplayLayer {
    Grid,
    Graph,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum AnimMsg {
    SetMousePan(Option<Point>),
    PanConst { dx: Option<f32>, dy: Option<f32> },
    PanDelta { dxy: Point },
    ZoomDelta { dz: f32 },
}

pub struct AnimHandlerThread {
    settings: Arc<AtomicCell<AnimSettings>>,
    screen_dims: Arc<AtomicCell<ScreenDims>>,
    initial_view: Arc<AtomicCell<View>>,
    mouse_pos: Arc<AtomicCell<Option<Point>>>,
    _join_handle: std::thread::JoinHandle<()>,
    msg_tx: channel::Sender<AnimMsg>,
}

impl AnimHandlerThread {
    #[allow(dead_code)]
    fn update_settings<F>(&self, f: F)
    where
        F: Fn(AnimSettings) -> AnimSettings,
    {
        let old = self.settings.load();
        let new = f(old);
        self.settings.store(new);
    }

    #[allow(dead_code)]
    fn anim_settings(&self) -> AnimSettings {
        self.settings.load()
    }

    fn set_mouse_pan(&self, origin: Option<Point>) {
        self.msg_tx.send(AnimMsg::SetMousePan(origin)).unwrap();
    }

    fn pan_const(&self, dx: Option<f32>, dy: Option<f32>) {
        let speed = self.settings.load().key_pan_speed;
        let dx = dx.map(|x| x * speed);
        let dy = dy.map(|x| x * speed);
        self.msg_tx.send(AnimMsg::PanConst { dx, dy }).unwrap();
    }

    fn pan_delta(&self, dxy: Point) {
        self.msg_tx.send(AnimMsg::PanDelta { dxy }).unwrap();
    }

    fn zoom_delta(&self, dz: f32) {
        self.msg_tx.send(AnimMsg::ZoomDelta { dz }).unwrap();
    }
}

fn anim_handler_thread<D: Into<ScreenDims>>(
    anim_handler: AnimHandler,
    screen_dims: D,
    view: Arc<AtomicCell<View>>,
) -> AnimHandlerThread {
    let mouse_pos = Arc::new(AtomicCell::new(None));

    let settings: Arc<AtomicCell<AnimSettings>> =
        Arc::new(AtomicCell::new(AnimSettings::default()));
    let initial_view = view.load();
    let initial_view = Arc::new(AtomicCell::new(initial_view));

    let screen_dims = Arc::new(AtomicCell::new(screen_dims.into()));

    let inner_settings = settings.clone();
    let inner_view = view;
    let inner_mouse_pos = mouse_pos.clone();
    let inner_dims = screen_dims.clone();

    let (msg_tx, msg_rx) = channel::unbounded::<AnimMsg>();

    let _join_handle = std::thread::spawn(move || {
        let update_delay = std::time::Duration::from_millis(5);
        let sleep_delay = std::time::Duration::from_micros(2500);

        let settings = inner_settings;
        let view = inner_view;
        let mouse_pos = inner_mouse_pos;
        let screen_dims = inner_dims;

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
                        let settings = settings.load();
                        anim.pan_delta(&settings, dxy);
                    }
                    AnimMsg::ZoomDelta { dz } => {
                        let view = view.load();
                        anim.zoom_delta(view.scale, dz)
                    }
                }
            }

            if last_update.elapsed() > update_delay {
                let dt = last_update.elapsed().as_secs_f32();
                let settings = settings.load();
                let pos = mouse_pos.load();
                let dims = screen_dims.load();
                anim.update_cell(&settings, &view, dims, pos, dt);
                last_update = std::time::Instant::now();
            } else {
                std::thread::sleep(sleep_delay);
            }
        }
    });

    AnimHandlerThread {
        _join_handle,
        mouse_pos,
        msg_tx,
        settings,
        screen_dims,
        initial_view,
    }
}

#[derive(Debug, Default, Clone)]
pub struct AnimHandler {
    // screen_dims: ScreenDims,
    mouse_pan_screen_origin: Option<Point>,
    view_anim_target: Option<View>,
    view_pan_const: Point,
    view_pan_delta: Point,
    view_scale_delta: f32,
}

impl AnimHandler {
    // fn set_screen_dims<D: Into<ScreenDims>>(&mut self, dims: D) {
    //     self.screen_dims = dims.into();
    // }

    fn update_cell(
        &mut self,
        settings: &AnimSettings,
        view: &AtomicCell<View>,
        screen_dims: ScreenDims,
        mouse_pos: Option<Point>,
        dt: f32,
    ) {
        let before = view.load();
        let new = self.update(settings, before, screen_dims, mouse_pos, dt);
        view.store(new);
    }

    fn update(
        &mut self,
        settings: &AnimSettings,
        mut view: View,
        screen_dims: ScreenDims,
        mouse_pos: Option<Point>,
        dt: f32,
    ) -> View {
        let pre_scale = view.scale;
        let pre_view = view;

        view.scale += view.scale * dt * self.view_scale_delta;

        if let Some(min_scale) = settings.min_view_scale {
            view.scale = view.scale.max(min_scale);
        }

        if let Some(max_scale) = settings.max_view_scale {
            view.scale = view.scale.min(max_scale);
        }

        let dxy = match (self.mouse_pan_screen_origin, mouse_pos) {
            (Some(origin), Some(mouse_pos)) => {
                (mouse_pos - origin) * settings.mouse_pan_mult * dt
            }
            _ => (self.view_pan_const + self.view_pan_delta) * dt,
        };

        /*
        if let Some(mouse) = mouse_pos {
            // view.center += dxy * view.scale;

            let mid = Point {
                x: screen_dims.width / 2.0,
                y: screen_dims.height / 2.0,
            };

            // let mid = Point::ZERO;

            // let mouse = mouse
            //     - Point {
            //         x: screen_dims.width / 2.0,
            //         y: screen_dims.height / 2.0,
            //     };

            // let mouse = mouse / 2.0;

            // let mouse = Point {
            //     x: mouse.x,
            //     y: -mouse.y,
            // };

            let center = pre_view.screen_point_to_world(screen_dims, mid);

            let mouse_world_pre =
                pre_view.screen_point_to_world(screen_dims, mouse);

            let pre_offset = mouse_world_pre - center;

            let mouse_world_post =
                view.screen_point_to_world(screen_dims, mouse);

            let post_offset = mouse_world_post - center;

            // println!("{:?}\t{:?}", mouse_world_pre, mouse_world_post);

            // let delta = post_offset - pre_offset;

            let delta = mouse_world_post - mouse_world_pre;
            // let delta = mouse_world_post - post_center;

            println!("{:?}", delta);

            // let delta = Point {
            //     x: delta.x * screen_dims.width,
            //     y: delta.y * screen_dims.height,
            // };

            // view.center =
            view.center += delta;
        } else {
            */
        view.center += dxy * view.scale;
        // }

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

    fn pan_delta(&mut self, settings: &AnimSettings, dxy: Point) {
        self.view_pan_delta += dxy;

        if let Some(max_speed) = settings.max_speed {
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
    min_view_scale: Option<f32>,
    max_view_scale: Option<f32>,
    max_speed: Option<f32>,
    key_pan_speed: f32,
    mouse_pan_mult: f32,
    wheel_zoom_base_speed: f32,
}

impl std::default::Default for AnimSettings {
    fn default() -> Self {
        Self {
            min_view_scale: Some(0.5),
            max_view_scale: None,
            max_speed: Some(600.0),
            key_pan_speed: 400.0,
            mouse_pan_mult: 1.0,
            wheel_zoom_base_speed: 0.45,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MainViewInput {
    ButtonMousePan,
    ButtonSelect,
    KeyPanUp,
    KeyPanRight,
    KeyPanDown,
    KeyPanLeft,
    KeyResetView,
    WheelZoom,
}

impl BindableInput for MainViewInput {
    fn default_binds() -> SystemInputBindings<Self> {
        use winit::event;
        use winit::event::VirtualKeyCode as Key;
        use MainViewInput as Input;

        let key_binds: FxHashMap<Key, Vec<KeyBind<Input>>> = [
            (Key::Up, Input::KeyPanUp),
            (Key::Down, Input::KeyPanDown),
            (Key::Left, Input::KeyPanLeft),
            (Key::Right, Input::KeyPanRight),
            (Key::Space, Input::KeyResetView),
        ]
        .iter()
        .copied()
        .map(|(k, i)| (k, vec![KeyBind::new(i)]))
        .collect::<FxHashMap<_, _>>();

        let mouse_binds: FxHashMap<
            event::MouseButton,
            Vec<MouseButtonBind<Input>>,
        > = [(
            event::MouseButton::Left,
            vec![
                MouseButtonBind::new(Input::ButtonMousePan),
                MouseButtonBind::new(Input::ButtonSelect),
            ],
        )]
        .iter()
        .cloned()
        .collect();

        let wheel_bind = Some(WheelBind::new(true, 0.45, Input::WheelZoom));

        SystemInputBindings::new(key_binds, mouse_binds, wheel_bind)
    }
}
