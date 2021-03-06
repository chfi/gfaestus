use crossbeam::channel;
use crossbeam::{
    atomic::AtomicCell,
    channel::{Receiver, Sender},
};

use std::sync::Arc;

use rgb::*;

use anyhow::Result;

use handlegraph::handle::NodeId;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::view::{ScreenDims, View};
use crate::{
    app::{node_flags::SelectionBuffer, NodeWidth},
    overlays::OverlayKind,
    vulkan::texture::GradientTexture,
};
use crate::{geometry::*, vulkan::render_pass::Framebuffers};

use crate::input::binds::{
    BindableInput, InputPayload, KeyBind, MouseButtonBind, SystemInput,
    SystemInputBindings, WheelBind,
};
use crate::input::MousePos;

use crate::vulkan::{
    context::VkContext,
    draw_system::nodes::{
        NodeIdBuffer, NodePipelines, NodeThemePipeline, NodeVertices,
    },
    GfaestusVk, SwapchainProperties,
};

use ash::vk;

pub mod view;

use view::*;

use super::{AppChannels, SharedState};

pub struct MainView {
    pub node_draw_system: crate::vulkan::draw_system::nodes::NodePipelines,
    pub node_id_buffer: NodeIdBuffer,
    pub selection_buffer: SelectionBuffer,

    node_width: Arc<NodeWidth>,

    anim_handler: AnimHandler,

    view_input_state: ViewInputState,

    // msg_tx: Sender<MainViewMsg>,
    // msg_rx: Receiver<MainViewMsg>,
    shared_state: SharedState,
    channels: AppChannels,

    move_delta: AtomicCell<Option<Point>>,
}

#[derive(Debug, Clone, Copy)]
pub enum MainViewMsg {
    GotoView(View),
}

impl MainView {
    pub fn new(
        app: &GfaestusVk,
        channels: AppChannels,
        shared_state: SharedState,
        node_width: Arc<NodeWidth>,
        node_count: usize,
    ) -> Result<Self> {
        let selection_buffer = SelectionBuffer::new(app, node_count)?;

        let swapchain_props = app.swapchain_props;
        let msaa_samples = app.msaa_samples;
        let render_pass = app.render_passes.nodes;

        let node_draw_system = NodePipelines::new(
            app,
            swapchain_props,
            msaa_samples,
            render_pass,
            selection_buffer.buffer,
        )?;

        let screen_dims = {
            let extent = swapchain_props.extent;
            ScreenDims {
                width: extent.width as f32,
                height: extent.height as f32,
            }
        };

        let node_id_buffer = NodeIdBuffer::new(
            &app,
            screen_dims.width as u32,
            screen_dims.height as u32,
        )?;

        let anim_handler = AnimHandler::new(
            shared_state.clone_view(),
            Point::ZERO,
            screen_dims,
        );

        let (msg_tx, msg_rx) = channel::unbounded::<MainViewMsg>();

        let main_view = Self {
            node_draw_system,
            node_id_buffer,
            selection_buffer,

            node_width,

            // view,
            anim_handler,

            view_input_state: Default::default(),

            // msg_tx,
            // msg_rx,
            move_delta: AtomicCell::new(None),

            shared_state,
            channels,
        };

        Ok(main_view)
    }

    pub fn main_view_msg_tx(&self) -> &Sender<MainViewMsg> {
        &self.channels.main_view_tx
    }

    pub fn main_view_msg_rx(&self) -> &Receiver<MainViewMsg> {
        &self.channels.main_view_rx
    }

    pub fn apply_msg(&self, msg: MainViewMsg) {
        match msg {
            MainViewMsg::GotoView(view) => {
                use std::time::Duration;

                let anim_def = AnimationDef {
                    kind: AnimationKind::Absolute,
                    order: AnimationOrder::Transform {
                        center: view.center,
                        scale: view.scale,
                    },
                    duration: Duration::from_millis(500),
                };
                self.anim_handler.send_anim_def(anim_def);
            }
        }
    }

    pub fn view(&self) -> View {
        self.shared_state.view.load()
    }

    pub fn set_initial_view(&self, center: Option<Point>, scale: Option<f32>) {
        let old_init_view = self.anim_handler.initial_view.load();
        let center = center.unwrap_or(old_init_view.center);
        let scale = scale.unwrap_or(old_init_view.scale);
        self.anim_handler.initial_view.store(View { center, scale });
    }

    pub fn reset_view(&self) {
        self.shared_state
            .set_view(self.anim_handler.initial_view.load());
    }

    pub fn set_view(&self, view: View) {
        self.shared_state.set_view(view);
    }

    pub fn node_id_buffer(&self) -> vk::Buffer {
        self.node_id_buffer.buffer
    }

    pub fn recreate_node_id_buffer(
        &mut self,
        app: &GfaestusVk,
        width: u32,
        height: u32,
    ) -> Result<()> {
        self.node_id_buffer.recreate(app, width, height)
    }

    pub fn read_nodes_around(&self, point: Point) -> FxHashSet<NodeId> {
        let x = point.x as u32;
        let y = point.y as u32;

        let min_x = if x < 40 { 0 } else { x - 40 };

        let min_y = if y < 40 { 0 } else { y - 40 };

        self.node_id_buffer.read_rect(
            self.node_draw_system.device(),
            min_x..=(x + 40),
            min_y..=(y + 40),
        )
    }

    pub fn read_node_id_at(&self, point: Point) -> Option<u32> {
        let x = point.x as u32;
        let y = point.y as u32;

        self.node_id_buffer
            .read(self.node_draw_system.device(), x, y)
    }

    pub fn draw_nodes_new(
        &mut self,
        cmd_buf: vk::CommandBuffer,
        render_pass: vk::RenderPass,
        framebuffers: &Framebuffers,
        screen_dims: [f32; 2],
        offset: Point,
        overlay: (usize, OverlayKind),
        color_scheme: &GradientTexture,
        use_overlay: bool,
    ) -> Result<()> {
        let view = self.shared_state.view();

        let node_width = {
            let min = self.node_width.min_node_width();
            let max = self.node_width.max_node_width();

            let min_scale = self.node_width.min_scale();
            let max_scale = self.node_width.max_scale();

            let norm_scale = (view.scale - min_scale) / (max_scale - min_scale);

            let easing_val =
                EasingExpoOut::value_at_normalized_time(norm_scale as f64)
                    as f32;

            let mut width = min + easing_val * (max - min);

            if view.scale > max_scale {
                width *= view.scale / (min_scale - max_scale);
            } else if view.scale < min_scale {
                width = min
            }
            width
        };

        if use_overlay {
            self.node_draw_system.draw_overlay_new(
                cmd_buf,
                render_pass,
                framebuffers,
                screen_dims,
                node_width,
                view,
                offset,
                overlay,
                color_scheme,
            )
        } else {
            self.node_draw_system.draw_themed(
                cmd_buf,
                render_pass,
                framebuffers,
                screen_dims,
                node_width,
                view,
                offset,
            )
        }

        /*
        let has_overlay = self.node_draw_system.has_overlay();

        if use_overlay && has_overlay {
            self.node_draw_system.draw_overlay(
                cmd_buf,
                render_pass,
                framebuffers,
                screen_dims,
                node_width,
                view,
                offset,
            )
        } else {
            self.node_draw_system.draw_themed(
                cmd_buf,
                render_pass,
                framebuffers,
                screen_dims,
                node_width,
                view,
                offset,
            )
        }
        */
    }

    pub fn draw_nodes(
        &self,
        cmd_buf: vk::CommandBuffer,
        render_pass: vk::RenderPass,
        framebuffers: &Framebuffers,
        screen_dims: [f32; 2],
        offset: Point,
        use_overlay: bool,
    ) -> Result<()> {
        let view = self.shared_state.view();

        let node_width = {
            let min = self.node_width.min_node_width();
            let max = self.node_width.max_node_width();

            let min_scale = self.node_width.min_scale();
            let max_scale = self.node_width.max_scale();

            // let norm_scale =
            //     1.0 - ((view.scale - min_scale) / (max_scale - min_scale));

            // let easing_val =
            //     EasingExpoIn::value_at_normalized_time(norm_scale as f64)
            //         as f32;

            let norm_scale = (view.scale - min_scale) / (max_scale - min_scale);

            let easing_val =
                EasingExpoOut::value_at_normalized_time(norm_scale as f64)
                    as f32;

            let mut width = min + easing_val * (max - min);

            if view.scale > max_scale {
                width *= view.scale / (min_scale - max_scale);
            } else if view.scale < min_scale {
                width = min
            }
            width
        };

        let has_overlay = self.node_draw_system.has_overlay();

        if use_overlay && has_overlay {
            self.node_draw_system.draw_overlay(
                cmd_buf,
                render_pass,
                framebuffers,
                screen_dims,
                node_width,
                view,
                offset,
            )
        } else {
            self.node_draw_system.draw_themed(
                cmd_buf,
                render_pass,
                framebuffers,
                screen_dims,
                node_width,
                view,
                offset,
            )
        }
    }

    pub fn update_node_selection(
        &mut self,
        new_selection: &FxHashSet<NodeId>,
    ) -> Result<()> {
        let device = self.node_draw_system.device();
        let selection = &mut self.selection_buffer;

        selection.update_selection(device, new_selection)
    }

    pub fn clear_node_selection(&mut self) -> Result<()> {
        let device = self.node_draw_system.device();
        let selection = &mut self.selection_buffer;

        selection.clear();
        selection.clear_buffer(device)
    }

    pub fn set_view_center(&self, center: Point) {
        let mut view = self.shared_state.view();
        view.center = center;
        self.shared_state.set_view(view);
    }

    pub fn set_view_scale(&self, scale: f32) {
        let mut view = self.shared_state.view();
        view.scale = scale;
        self.shared_state.set_view(view);
    }

    pub fn update_view_animation<D: Into<ScreenDims>>(
        &self,
        screen_dims: D,
        mouse_pos: Point,
    ) {
        let screen_dims = screen_dims.into();
        let view = self.shared_state.view();

        let mouse_screen = mouse_pos;
        let mouse_world = view.screen_point_to_world(screen_dims, mouse_screen);

        if let Some(anim_def) = self.view_input_state.animation_def(
            view,
            screen_dims,
            mouse_screen,
            mouse_world,
        ) {
            self.anim_handler.send_anim_def(anim_def);
        }
    }

    pub fn apply_input<Dims: Into<ScreenDims>>(
        &self,
        screen_dims: Dims,
        mouse_pos: Point,
        input: SystemInput<MainViewInput>,
    ) {
        use MainViewInput as In;
        let payload = input.payload();

        match input {
            SystemInput::Keyboard { state, .. } => {
                let pressed = state.pressed();

                match payload {
                    In::KeyPanUp => {
                        self.view_input_state.key_pan.set_up(pressed);
                    }
                    In::KeyPanRight => {
                        self.view_input_state.key_pan.set_right(pressed);
                    }
                    In::KeyPanDown => {
                        self.view_input_state.key_pan.set_down(pressed);
                    }
                    In::KeyPanLeft => {
                        self.view_input_state.key_pan.set_left(pressed);
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
                            if self.shared_state.hover_node().is_some() {
                                let view = self.shared_state.view();
                                let mouse_world = view.screen_point_to_world(
                                    screen_dims,
                                    mouse_pos,
                                );

                                self.move_delta.store(Some(mouse_world));
                            } else if !self.shared_state.is_started_mouse_rect()
                            {
                                let view = self.shared_state.view();
                                let mouse_world = view.screen_point_to_world(
                                    screen_dims,
                                    mouse_pos,
                                );

                                self.view_input_state.start_click_and_drag_pan(
                                    view,
                                    mouse_world,
                                );
                            }
                        } else {
                            if let Some(start) = self.move_delta.load() {
                                use crate::app::AppMsg;

                                let view = self.shared_state.view();
                                let mouse_world = view.screen_point_to_world(
                                    screen_dims,
                                    mouse_pos,
                                );

                                let delta = mouse_world - start;

                                self.channels
                                    .app_tx
                                    .send(AppMsg::TranslateSelected(delta))
                                    .unwrap();

                                self.move_delta.store(None);
                            }
                            self.view_input_state.mouse_released();
                        }
                    }
                    In::ButtonSelect => {
                        use crate::app::AppMsg;
                        use crate::app::Select;

                        let selected_node = self
                            .read_node_id_at(pos)
                            .map(|nid| NodeId::from(nid as u64));

                        if let Some(node) = selected_node {
                            self.channels
                                .app_tx
                                .send(AppMsg::Selection(Select::One {
                                    node,
                                    clear: false,
                                }))
                                .unwrap();
                        }
                    }

                    In::ButtonRectangleSelect => {
                        use crate::app::AppMsg;
                        use crate::app::Select;

                        if pressed {
                            let view = self.shared_state.view();

                            let mouse_pos_world = view
                                .screen_point_to_world(screen_dims, mouse_pos);

                            self.shared_state.start_mouse_rect();
                            // self.rectangle_select_start
                            //     .store(Some(mouse_pos_world));
                            // self.rectangle_select_start.store(Some(mouse_pos));
                        } else {
                            // if let Some(start) =
                            //     self.rectangle_select_start.load()
                            if self.shared_state.is_started_mouse_rect() {
                                // let view = self.shared_state.view();

                                // let mouse_pos_world = view
                                //     .screen_point_to_world(
                                //         screen_dims,
                                //         mouse_pos,
                                //     );

                                // let rect = Rect::new(start, mouse_pos_world);

                                if let Some(rect) =
                                    self.shared_state.close_mouse_rect_world()
                                {
                                    self.channels
                                        .app_tx
                                        .send(AppMsg::RectSelect(rect))
                                        .unwrap();
                                }

                                /*
                                let end = mouse_pos;

                                let min = Point {
                                    x: start.x.min(end.x),
                                    y: start.y.min(end.y),
                                };

                                let max = Point {
                                    x: start.x.max(end.x),
                                    y: start.y.max(end.y),
                                };

                                let x_range = (min.x as u32)..=(max.x as u32);
                                let y_range = (min.y as u32)..=(max.y as u32);

                                let selection = self.node_id_buffer.read_rect(
                                    self.node_draw_system.device(),
                                    x_range,
                                    y_range,
                                );

                                app_msg_tx
                                    .send(AppMsg::Selection(Select::Many {
                                        nodes: selection,
                                        clear: false,
                                    }))
                                    .unwrap();
                                */
                            }

                            // self.rectangle_select_start.store(None);
                        }
                    }
                    _ => (),
                }
            }
            SystemInput::Wheel { delta, .. } => {
                if let In::WheelZoom = payload {
                    self.view_input_state.scroll_zoom(
                        self.shared_state.view(),
                        mouse_pos,
                        delta,
                    );
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MainViewInput {
    ButtonMousePan,
    ButtonSelect,
    ButtonRectangleSelect,
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

        let shift_mod = winit::event::ModifiersState::SHIFT;

        let mouse_binds: FxHashMap<
            event::MouseButton,
            Vec<MouseButtonBind<Input>>,
        > = [
            (
                event::MouseButton::Left,
                vec![
                    MouseButtonBind::new(Input::ButtonMousePan),
                    MouseButtonBind::with_modifiers(
                        Input::ButtonRectangleSelect,
                        shift_mod,
                    ),
                ],
            ),
            (
                event::MouseButton::Right,
                vec![MouseButtonBind::new(Input::ButtonSelect)],
            ),
        ]
        .iter()
        .cloned()
        .collect();

        let wheel_bind = Some(WheelBind::new(true, 0.45, Input::WheelZoom));

        SystemInputBindings::new(key_binds, mouse_binds, wheel_bind)
    }
}
