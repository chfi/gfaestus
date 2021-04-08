use std::sync::Arc;

#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use anyhow::Result;

use rustc_hash::FxHashMap;

use crossbeam::channel;
use parking_lot::Mutex;

mod theme_editor;

use theme_editor::*;

use crate::geometry::*;
use crate::view::View;
use crate::{app::RenderConfigOpts, vulkan::render_pass::Framebuffers};

use crate::input::binds::{
    BindableInput, InputPayload, KeyBind, MouseButtonBind, SystemInput,
    SystemInputBindings, WheelBind,
};
use crate::input::MousePos;

use super::theme::{ThemeDef, ThemeId};

use crate::app::settings::AppConfigState;

use crate::vulkan::{
    context::VkContext,
    draw_system::gui::{GuiPipeline, GuiVertex, GuiVertices},
    GfaestusVk, SwapchainProperties,
};

use ash::vk;

pub struct GfaestusGui {
    ctx: egui::CtxRef,
    frame_input: FrameInput,
    enabled_ui_elements: EnabledUiElements,

    // gui_draw_system: GuiDrawSystem,
    pub gui_draw_system: GuiPipeline,

    hover_node_id: Option<NodeId>,
    selected_node: NodeSelection,

    graph_stats: GraphStatsUi,
    view_info: ViewInfoUi,
    frame_rate_box: FrameRateBox,

    render_config_ui: RenderConfigUi,

    theme_editor: ThemeEditorWindow,

    app_cfg_tx: channel::Sender<AppConfigState>,

    overlay_enabled: bool,
}

#[derive(Debug, Clone)]
enum NodeSelection {
    None,
    One { info: NodeInfo },
    Many { count: usize },
}

impl NodeSelection {
    fn some_selection(&self) -> bool {
        match self {
            NodeSelection::None => false,
            NodeSelection::One { .. } => true,
            NodeSelection::Many { .. } => true,
        }
    }
}

impl std::default::Default for NodeSelection {
    fn default() -> Self {
        NodeSelection::None
    }
}

#[derive(Debug, Default, Clone)]
struct FrameInput {
    events: Vec<egui::Event>,
    scroll_delta: f32,
}

impl FrameInput {
    fn into_raw_input(&mut self) -> egui::RawInput {
        let mut raw_input = egui::RawInput::default();
        raw_input.events = std::mem::take(&mut self.events);
        raw_input.scroll_delta = egui::Vec2 {
            x: 0.0,
            y: self.scroll_delta,
        };
        self.scroll_delta = 0.0;

        raw_input
    }
}

#[derive(Debug, Clone, Copy)]
struct EnabledUiElements {
    egui_inspection_ui: bool,
    egui_settings_ui: bool,
    egui_memory_ui: bool,

    frame_rate: bool,
    graph_stats: bool,
    view_info: bool,
    selected_node: bool,

    theme_editor: bool,
}

impl std::default::Default for EnabledUiElements {
    fn default() -> Self {
        Self {
            egui_inspection_ui: false,
            egui_settings_ui: false,
            egui_memory_ui: false,

            frame_rate: true,
            graph_stats: true,
            view_info: false,
            selected_node: true,

            theme_editor: false,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct RenderConfigUi {
    nodes_color: bool,
    nodes_mask: bool,

    selection_edge_detect: bool,
    selection_edge_blur: bool,
    selection_edge: bool,

    lines: bool,
    // gui: bool,
}

impl std::default::Default for RenderConfigUi {
    fn default() -> Self {
        Self {
            nodes_color: true,
            nodes_mask: true,

            selection_edge_detect: true,
            selection_edge_blur: true,
            selection_edge: true,

            lines: true,
        }
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct NodeInfo {
    node_id: NodeId,
    len: usize,
    degree: (usize, usize),
    coverage: usize,
}

#[derive(Debug, Default, Clone, Copy)]
struct FrameRateBox {
    fps: f32,
    frame_time: f32,
    frame: usize,
}

#[derive(Debug, Default, Clone, Copy)]
struct ViewInfoUi {
    position: Point,
    view: View,
    mouse_screen: Point,
    mouse_world: Point,
}

#[derive(Debug, Default, Clone, Copy)]
struct GraphStatsUi {
    position: Point,
    stats: GraphStats,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub path_count: usize,
    pub total_len: usize,
}

impl GfaestusGui {
    pub fn new(
        app: &GfaestusVk,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
    ) -> Result<(GfaestusGui, channel::Receiver<AppConfigState>)> {
        // let gui_draw_system = GuiDrawSystem::new(gfx_queue, subpass);
        let gui_draw_system = GuiPipeline::new(app, msaa_samples, render_pass)?;

        let ctx = egui::CtxRef::default();

        let mut style: egui::Style = (*ctx.style()).clone();
        style.visuals.window_corner_radius = 0.0;
        ctx.set_style(style);

        let font_defs = {
            use egui::FontFamily as Family;
            use egui::TextStyle as Style;

            let mut font_defs = egui::FontDefinitions::default();
            let fam_size = &mut font_defs.family_and_size;

            fam_size.insert(Style::Small, (Family::Proportional, 12.0));
            fam_size.insert(Style::Body, (Family::Proportional, 16.0));
            fam_size.insert(Style::Button, (Family::Proportional, 18.0));
            fam_size.insert(Style::Heading, (Family::Proportional, 22.0));
            font_defs
        };
        ctx.set_fonts(font_defs);

        let hover_node_id = None;

        let graph_stats = GraphStatsUi {
            position: Point { x: 12.0, y: 40.0 },
            ..GraphStatsUi::default()
        };

        let view_info = ViewInfoUi {
            position: Point { x: 12.0, y: 140.0 },
            ..ViewInfoUi::default()
        };

        let frame_rate_box = FrameRateBox {
            fps: 0.0,
            frame_time: 0.0,
            frame: 0,
        };

        let (app_cfg_tx, app_cfg_rx) = channel::unbounded::<AppConfigState>();

        Ok((
            Self {
                ctx,
                frame_input: FrameInput::default(),
                enabled_ui_elements: EnabledUiElements::default(),

                hover_node_id,
                selected_node: NodeSelection::default(),

                gui_draw_system,
                graph_stats,
                view_info,
                frame_rate_box,
                render_config_ui: Default::default(),

                theme_editor: ThemeEditorWindow::new(app_cfg_tx.clone()),

                app_cfg_tx,

                overlay_enabled: false,
            },
            app_cfg_rx,
        ))
    }

    pub fn update_theme_editor(&mut self, id: ThemeId, theme: &ThemeDef) {
        self.theme_editor.update_theme(id, theme);
    }

    pub fn set_dark_mode(&self) {
        let mut style: egui::Style = (*self.ctx.style()).clone();
        style.visuals = egui::style::Visuals::dark();
        style.visuals.window_corner_radius = 0.0;
        self.ctx.set_style(style);
    }

    pub fn set_light_mode(&self) {
        let mut style: egui::Style = (*self.ctx.style()).clone();
        style.visuals = egui::style::Visuals::light();
        style.visuals.window_corner_radius = 0.0;
        self.ctx.set_style(style);
    }

    pub fn set_frame_rate(&mut self, frame: usize, fps: f32, frame_time: f32) {
        self.frame_rate_box.frame = frame;
        self.frame_rate_box.fps = fps;
        self.frame_rate_box.frame_time = frame_time;
    }

    pub fn set_graph_stats(&mut self, stats: GraphStats) {
        self.graph_stats.stats = stats;
    }

    pub fn set_view_info_view(&mut self, view: View) {
        self.view_info.view = view;
    }

    pub fn set_overlay_state(&mut self, to: bool) {
        self.overlay_enabled = to;
    }

    pub fn set_view_info_mouse(
        &mut self,
        mouse_screen: Point,
        mouse_world: Point,
    ) {
        self.view_info.mouse_screen = mouse_screen;
        self.view_info.mouse_world = mouse_world;
    }

    pub fn set_hover_node(&mut self, node: Option<NodeId>) {
        self.hover_node_id = node;
    }

    pub fn no_selection(&mut self) {
        self.selected_node = NodeSelection::None;
    }

    pub fn one_selection(
        &mut self,
        node_id: NodeId,
        len: usize,
        degree: (usize, usize),
        coverage: usize,
    ) {
        let info = NodeInfo {
            node_id,
            len,
            degree,
            coverage,
        };

        self.selected_node = NodeSelection::One { info };
    }

    pub fn many_selection(&mut self, count: usize) {
        self.selected_node = NodeSelection::Many { count };
    }

    pub fn selected_node(&self) -> Option<NodeId> {
        match self.selected_node {
            NodeSelection::None => None,
            NodeSelection::One { info } => Some(info.node_id),
            NodeSelection::Many { count } => None,
        }
    }

    fn graph_stats(&self, pos: Point) {
        let stats = self.graph_stats.stats;

        egui::Area::new("graph_summary_stats").fixed_pos(pos).show(
            &self.ctx,
            |ui| {
                ui.label(format!("nodes: {}", stats.node_count));
                ui.label(format!("edges: {}", stats.edge_count));
                ui.label(format!("paths: {}", stats.path_count));
                ui.label(format!("total length: {}", stats.total_len));
            },
        );
    }

    fn view_info(&self, pos: Point) {
        let info = self.view_info;

        egui::Area::new("view_mouse_info").fixed_pos(pos).show(
            &self.ctx,
            |ui| {
                ui.label(format!(
                    "view origin: x: {:6}\ty: {:6}",
                    info.view.center.x, info.view.center.y
                ));
                ui.label(format!("view scale: {}", info.view.scale));
                ui.label(format!(
                    "mouse world:  {:6}\t{:6}",
                    info.mouse_world.x, info.mouse_world.y
                ));
                ui.label(format!(
                    "mouse screen: {:6}\t{:6}",
                    info.mouse_screen.x, info.mouse_screen.y
                ));
            },
        );
    }

    pub fn set_render_config(
        &mut self,
        nodes_color: bool,
        outline: bool,
        edge_detect: bool,
        edge_blur: bool,
    ) {
        self.render_config_ui.nodes_color = nodes_color;

        self.render_config_ui.selection_edge = outline;

        self.render_config_ui.selection_edge_detect = edge_detect;
        self.render_config_ui.selection_edge_blur = edge_blur;
    }

    fn render_config_info(&self, pos: Point) {
        let cfg_info = self.render_config_ui;

        egui::Area::new("render_config_info_ui")
            .fixed_pos(pos)
            .show(&self.ctx, |ui| {
                ui.label(format!("nodes_color: {}", cfg_info.nodes_color));
                ui.label(format!("nodes_mask: {}", cfg_info.nodes_mask));

                ui.label(format!(
                    "selection_edge_detect: {}",
                    cfg_info.selection_edge_detect
                ));
                ui.label(format!(
                    "selection_edge_blur: {}",
                    cfg_info.selection_edge_blur
                ));

                ui.label(format!("lines: {}", cfg_info.lines));
            });
    }

    pub fn menu_bar(&mut self) {
        let ctx = &self.ctx;
        let app_chn = &self.app_cfg_tx;
        let overlay_enabled = &self.overlay_enabled;
        let enabled = &mut self.enabled_ui_elements;

        egui::TopPanel::top("gfaestus_top_menu_bar").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(enabled.theme_editor, "Theme Editor")
                    .clicked()
                {
                    enabled.theme_editor = !enabled.theme_editor;
                }

                if ui.selectable_label(enabled.frame_rate, "FPS").clicked() {
                    enabled.frame_rate = !enabled.frame_rate;
                }

                if ui.selectable_label(*overlay_enabled, "Overlay").clicked() {
                    app_chn.send(AppConfigState::ToggleOverlay).unwrap();
                }
            });
        });
    }

    pub fn begin_frame(&mut self, screen_rect: Option<Point>) {
        let mut raw_input = self.frame_input.into_raw_input();
        let screen_rect = screen_rect.map(|p| egui::Rect {
            min: Point::ZERO.into(),
            max: p.into(),
        });
        raw_input.screen_rect = screen_rect;

        self.ctx.begin_frame(raw_input);

        let scr = self.ctx.input().screen_rect();

        self.menu_bar();

        if let Some(node_id) = self.hover_node_id {
            egui::containers::popup::show_tooltip_text(
                &self.ctx,
                node_id.0.to_string(),
            )
        }

        if self.selected_node.some_selection() {
            let top_left = Point {
                x: 0.0,
                y: 0.80 * scr.max.y,
            };
            let bottom_right = Point {
                x: 200.0,
                y: scr.max.y,
            };

            let rect = egui::Rect {
                min: top_left.into(),
                max: bottom_right.into(),
            };

            egui::Window::new("node_select_info")
                .fixed_rect(rect)
                .title_bar(false)
                .show(&self.ctx, |ui| {
                    ui.expand_to_include_rect(rect);

                    match &self.selected_node {
                        NodeSelection::None => (),
                        NodeSelection::One { info } => {
                            let node_info = info;

                            let label = format!(
                                "Selected node: {}",
                                node_info.node_id.0
                            );
                            ui.label(label);
                            let lb_len = format!("Length: {}", node_info.len);
                            let lb_deg = format!(
                                "Degree: ({}, {})",
                                node_info.degree.0, node_info.degree.1
                            );
                            let lb_cov =
                                format!("Coverage: {}", node_info.coverage);

                            ui.label(lb_len);
                            ui.label(lb_deg);
                            ui.label(lb_cov);
                        }
                        NodeSelection::Many { count } => {
                            ui.label(format!("Selected {} nodes", count));
                        }
                    }
                });
        }

        if self.enabled_ui_elements.graph_stats {
            self.graph_stats(self.graph_stats.position);
        }

        if self.enabled_ui_elements.view_info {
            self.view_info(self.view_info.position);
        }

        if self.enabled_ui_elements.frame_rate {
            let p0 = Point {
                x: 0.8 * scr.max.x,
                y: 30.0,
            };

            let p1 = Point {
                x: scr.max.x,
                y: 100.0,
            };

            egui::Window::new("mouse_over_egui")
                .fixed_rect(egui::Rect {
                    min: p0.into(),
                    max: p1.into(),
                })
                .title_bar(false)
                .show(&self.ctx, |ui| {
                    ui.label(format!("FPS: {:.2}", self.frame_rate_box.fps));
                    ui.label(format!(
                        "update time: {:.2} ms",
                        self.frame_rate_box.frame_time
                    ));
                });
        }

        // if self.enabled_ui_elements.render_config {
        /*
        self.render_config_info(Point {
            x: 0.8 * scr.max.x,
            y: 0.8 * scr.max.y,
        });
        */
        // }

        if self.enabled_ui_elements.egui_inspection_ui {
            egui::Window::new("egui_inspection_ui_window")
                .show(&self.ctx, |ui| self.ctx.inspection_ui(ui));
        }

        if self.enabled_ui_elements.egui_settings_ui {
            egui::Window::new("egui_settings_ui_window")
                .show(&self.ctx, |ui| self.ctx.settings_ui(ui));
        }

        if self.enabled_ui_elements.egui_memory_ui {
            egui::Window::new("egui_memory_ui_window")
                .show(&self.ctx, |ui| self.ctx.memory_ui(ui));
        }

        self.theme_editor
            .show(&self.ctx, &mut self.enabled_ui_elements.theme_editor);
    }

    pub fn toggle_egui_inspection_ui(&mut self) {
        self.enabled_ui_elements.egui_inspection_ui =
            !self.enabled_ui_elements.egui_inspection_ui;
    }

    pub fn toggle_egui_settings_ui(&mut self) {
        self.enabled_ui_elements.egui_settings_ui =
            !self.enabled_ui_elements.egui_settings_ui;
    }

    pub fn toggle_egui_memory_ui(&mut self) {
        self.enabled_ui_elements.egui_memory_ui =
            !self.enabled_ui_elements.egui_memory_ui;
    }

    pub fn pointer_over_gui(&self) -> bool {
        self.ctx.is_pointer_over_area()
    }

    pub fn upload_texture(&mut self, app: &GfaestusVk) -> Result<()> {
        let egui_tex = self.ctx.texture();
        if egui_tex.version != self.gui_draw_system.texture_version() {
            self.gui_draw_system.upload_texture(
                app,
                app.transient_command_pool,
                app.graphics_queue,
                &egui_tex,
            )?;
        }

        Ok(())
    }

    pub fn upload_vertices(
        &mut self,
        app: &GfaestusVk,
        meshes: &[egui::ClippedMesh],
    ) -> Result<()> {
        self.gui_draw_system.vertices.upload_meshes(app, meshes)
    }

    pub fn draw(
        &self,
        cmd_buf: vk::CommandBuffer,
        render_pass: vk::RenderPass,
        framebuffers: &Framebuffers,
        screen_dims: [f32; 2],
    ) -> Result<()> {
        self.gui_draw_system.draw(
            cmd_buf,
            render_pass,
            framebuffers,
            screen_dims,
        )
    }

    pub fn end_frame(&self) -> Vec<egui::ClippedMesh> {
        let (_output, shapes) = self.ctx.end_frame();
        self.ctx.tessellate(shapes)
    }

    /*
    fn draw_tessellated(&mut self, app: &GfaestusVk, clipped_meshes: &[egui::ClippedMesh]) -> Result<()> {
        let egui_tex = self.ctx.texture();
        if egui_tex.version != self.gui_draw_system.texture_version() {
            self.gui_draw_system.upload_texture(
                app,
                app.transient_command_pool,
                app.graphics_queue,
                egui_tex,
            )?;
        }
    }
    */

    /*
    fn draw_tessellated(
        &self,
        dynamic_state: &DynamicState,
        clipped_meshes: &[egui::ClippedMesh],
    ) -> Result<(Vec<AutoCommandBuffer>, Option<Box<dyn GpuFuture>>)> {
        let egui_tex = self.ctx.texture();
        let tex_future =
            self.gui_draw_system.upload_texture(&egui_tex).transpose()?;
        let cmd_buf = self
            .gui_draw_system
            .draw_egui_ctx(dynamic_state, clipped_meshes)?;

        Ok((cmd_buf, tex_future))
    }

    pub fn end_frame_and_draw(
        &self,
        dynamic_state: &DynamicState,
    ) -> Option<Result<(Vec<AutoCommandBuffer>, Option<Box<dyn GpuFuture>>)>>
    {
        let (_output, shapes) = self.ctx.end_frame();
        let clipped_meshes = self.ctx.tessellate(shapes);

        if clipped_meshes.is_empty() {
            return None;
        }

        Some(self.draw_tessellated(dynamic_state, &clipped_meshes))
    }

    */

    pub fn push_event(&mut self, event: egui::Event) {
        self.frame_input.events.push(event);
    }

    pub fn apply_input(
        &mut self,
        app_msg_tx: &channel::Sender<crate::app::AppMsg>,
        cfg_msg_tx: &channel::Sender<crate::app::AppConfigMsg>,
        input: SystemInput<GuiInput>,
    ) {
        use GuiInput as In;
        let payload = input.payload();

        match input {
            SystemInput::Keyboard { state, .. } => {
                if state.pressed() {
                    match payload {
                        GuiInput::KeyEguiInspectionUi => {
                            self.toggle_egui_inspection_ui();
                        }
                        GuiInput::KeyEguiSettingsUi => {
                            self.toggle_egui_settings_ui();
                        }
                        GuiInput::KeyEguiMemoryUi => {
                            self.toggle_egui_memory_ui();
                        }
                        GuiInput::KeyToggleRender(opt) => {
                            use crate::app::AppConfigMsg as Msg;
                            use crate::app::RenderConfigOpts as Opts;

                            let cfg_msg = match opt {
                                Opts::SelOutlineEdge => {
                                    Msg::ToggleSelectionEdgeDetect
                                }
                                Opts::SelOutlineBlur => {
                                    Msg::ToggleSelectionEdgeBlur
                                }
                                Opts::SelOutline => Msg::ToggleSelectionOutline,
                                Opts::NodesColor => Msg::ToggleNodesColor,
                            };

                            cfg_msg_tx.send(cfg_msg).unwrap();
                        }
                        _ => (),
                    }
                }
            }
            SystemInput::MouseButton { pos, state, .. } => {
                let pressed = state.pressed();

                let button = match payload {
                    GuiInput::ButtonLeft => Some(egui::PointerButton::Primary),
                    GuiInput::ButtonRight => {
                        Some(egui::PointerButton::Secondary)
                    }

                    _ => None,
                };

                if let Some(button) = button {
                    let egui_event = egui::Event::PointerButton {
                        pos: pos.into(),
                        button,
                        pressed,
                        modifiers: Default::default(),
                    };

                    self.push_event(egui_event);
                }
            }
            SystemInput::Wheel { delta, .. } => {
                if let In::WheelScroll = payload {
                    self.frame_input.scroll_delta = delta;
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GuiInput {
    KeyEguiInspectionUi,
    KeyEguiSettingsUi,
    KeyEguiMemoryUi,
    ButtonLeft,
    ButtonRight,
    WheelScroll,
    KeyToggleRender(RenderConfigOpts),
}

impl BindableInput for GuiInput {
    fn default_binds() -> SystemInputBindings<Self> {
        use winit::event;
        use winit::event::VirtualKeyCode as Key;
        use GuiInput as Input;

        let key_binds: FxHashMap<Key, Vec<KeyBind<Input>>> = [
            (Key::F1, Input::KeyEguiInspectionUi),
            (Key::F2, Input::KeyEguiSettingsUi),
            (Key::F3, Input::KeyEguiMemoryUi),
            (
                Key::Key1,
                Input::KeyToggleRender(RenderConfigOpts::SelOutlineEdge),
            ),
            (
                Key::Key2,
                Input::KeyToggleRender(RenderConfigOpts::SelOutlineBlur),
            ),
            (
                Key::Key3,
                Input::KeyToggleRender(RenderConfigOpts::SelOutline),
            ),
            (
                Key::Key4,
                Input::KeyToggleRender(RenderConfigOpts::NodesColor),
            ),
        ]
        .iter()
        .copied()
        .map(|(k, i)| (k, vec![KeyBind::new(i)]))
        .collect::<FxHashMap<_, _>>();

        let mouse_binds: FxHashMap<
            event::MouseButton,
            Vec<MouseButtonBind<Input>>,
        > = [
            (
                event::MouseButton::Left,
                vec![MouseButtonBind::new(Input::ButtonLeft)],
            ),
            (
                event::MouseButton::Right,
                vec![MouseButtonBind::new(Input::ButtonRight)],
            ),
        ]
        .iter()
        .cloned()
        .collect();

        let wheel_bind = Some(WheelBind::new(false, 1.0, Input::WheelScroll));

        SystemInputBindings::new(key_binds, mouse_binds, wheel_bind)
    }
}
