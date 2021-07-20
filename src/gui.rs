use std::{path::PathBuf, sync::Arc};

use clipboard::{ClipboardContext, ClipboardProvider};
use futures::executor::ThreadPool;
use gfa::gfa::Path;
use gluon::vm::frunk_core::hlist::Sculptor;
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

use crossbeam::{atomic::AtomicCell, channel};
use parking_lot::Mutex;

use crate::{
    annotations::{Annotations, Gff3Record, Gff3Records},
    app::{AppChannels, AppMsg, AppSettings, SharedState},
    gluon::repl::GluonRepl,
    graph_query::GraphQueryWorker,
    vulkan::{render_pass::Framebuffers, texture::Gradients},
};
use crate::{
    app::{NodeWidth, OverlayState},
    geometry::*,
};

use crate::overlays::{OverlayData, OverlayKind};
use crate::{gluon::GraphHandle, view::View};

use crate::graph_query::GraphQuery;

use crate::input::binds::{
    BindableInput, InputPayload, KeyBind, MouseButtonBind, SystemInput,
    SystemInputBindings, WheelBind,
};
use crate::input::MousePos;

// use super::theme::{ThemeDef, ThemeId};

use crate::vulkan::{
    context::VkContext,
    draw_system::gui::{GuiPipeline, GuiVertex, GuiVertices},
    GfaestusVk, SwapchainProperties,
};

use ash::{extensions::khr::PushDescriptor, vk};

pub mod widgets;
pub mod windows;

use widgets::*;
use windows::*;

pub struct Gui {
    ctx: egui::CtxRef,
    frame_input: FrameInput,

    shared_state: SharedState,
    settings: AppSettings,

    pub draw_system: GuiPipeline,

    hover_node_id: Option<NodeId>,

    windows_active_view: FxHashMap<Windows, Views>,

    open_windows: OpenWindows,

    view_state: AppViewState,

    gui_msg_rx: crossbeam::channel::Receiver<GuiMsg>,
    gui_msg_tx: crossbeam::channel::Sender<GuiMsg>,

    app_msg_tx: crossbeam::channel::Sender<AppMsg>,

    menu_bar: MenuBar,

    dropped_file: Arc<std::sync::Mutex<Option<PathBuf>>>,

    thread_pool: Arc<ThreadPool>,

    annotations: Annotations,

    clipboard_ctx: ClipboardContext,

    gff3_list: Gff3RecordList,

    path_picker_source: PathPickerSource,

    file_picker: FilePicker,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Windows {
    Settings,

    // Console,
    FPS,
    GraphStats,
    // ViewInfo,
    Nodes,
    NodeDetails,

    Paths,

    Themes,
    Overlays,

    EguiInspection,
    EguiSettings,
    EguiMemory,
}

pub struct ViewStateChannel<T, U>
where
    U: Send + Sync,
{
    state: T,
    tx: crossbeam::channel::Sender<U>,
    rx: crossbeam::channel::Receiver<U>,
}

impl<T, U> std::default::Default for ViewStateChannel<T, U>
where
    T: Default,
    U: Send + Sync,
{
    fn default() -> Self {
        let (tx, rx) = crossbeam::channel::unbounded::<U>();
        let state = T::default();

        Self { state, tx, rx }
    }
}

impl<T, U> ViewStateChannel<T, U>
where
    U: Send + Sync,
{
    pub fn new(state: T) -> Self {
        let (tx, rx) = crossbeam::channel::unbounded::<U>();
        Self { state, tx, rx }
    }

    pub fn send(&self, msg: U) {
        self.tx.send(msg).unwrap();
    }

    pub fn clone_tx(&self) -> crossbeam::channel::Sender<U> {
        self.tx.clone()
    }

    pub fn apply_received<F>(&mut self, f: F)
    where
        F: for<'a> Fn(&'a mut T, U),
    {
        while let Ok(msg) = self.rx.try_recv() {
            f(&mut self.state, msg);
        }
    }
}

pub struct AppViewState {
    settings: MainViewSettings,
    // settings: (),
    fps: ViewStateChannel<FrameRate, FrameRateMsg>,

    graph_stats: ViewStateChannel<GraphStats, GraphStatsMsg>,

    // view_info: ViewStateChannel<ViewInfo, ViewInfoMsg>,
    node_list: ViewStateChannel<NodeList, NodeListMsg>,
    node_details: ViewStateChannel<NodeDetails, NodeDetailsMsg>,

    path_list: ViewStateChannel<PathList, PathListMsg>,
    path_details: ViewStateChannel<PathDetails, ()>,
    // path_details: PathList,

    // theme_editor: ThemeEditor,
    // theme_list: ThemeList,
    overlay_creator: ViewStateChannel<OverlayCreator, OverlayCreatorMsg>,
    overlay_list: ViewStateChannel<OverlayList, OverlayListMsg>,
    // overlay_editor: OverlayEditor,
    // overlay_list: OverlayList,
    repl_window: ViewStateChannel<ReplWindow, ()>,
}

impl AppViewState {
    pub fn new(
        graph_query: &GraphQuery,
        settings: &AppSettings,
        shared_state: &SharedState,
        overlay_state: OverlayState,
        dropped_file: Arc<std::sync::Mutex<Option<PathBuf>>>,
        thread_pool: &ThreadPool,
        repl: GluonRepl,
    ) -> Self {
        // let fps = ViewStateChannel::<FrameRate, FrameRateMsg>::default();

        let graph = graph_query.graph();

        let stats = GraphStats {
            node_count: graph.node_count(),
            edge_count: graph.edge_count(),
            path_count: graph.path_count(),
            total_len: graph.total_length(),
        };

        let settings =
            MainViewSettings::new(settings, shared_state.clone_edges_enabled());

        let node_details_state = NodeDetails::default();
        let node_id_cell = node_details_state.node_id_cell().clone();
        let node_details = ViewStateChannel::<NodeDetails, NodeDetailsMsg>::new(
            node_details_state,
        );

        let node_list_state =
            NodeList::new(graph_query, 15, node_id_cell.clone());
        let node_list =
            ViewStateChannel::<NodeList, NodeListMsg>::new(node_list_state);

        let path_details_state = PathDetails::default();
        let path_id_cell =
            path_details_state.path_details.path_id_cell().clone();
        let path_details =
            ViewStateChannel::<PathDetails, ()>::new(path_details_state);

        let path_list_state = PathList::new(graph_query, 15, path_id_cell);
        let path_list =
            ViewStateChannel::<PathList, PathListMsg>::new(path_list_state);

        let overlay_list_state = OverlayList::new(overlay_state);
        let overlay_list = ViewStateChannel::<OverlayList, OverlayListMsg>::new(
            overlay_list_state,
        );

        let overlay_creator_state = OverlayCreator::new(dropped_file).unwrap();
        let overlay_creator = ViewStateChannel::<
            OverlayCreator,
            OverlayCreatorMsg,
        >::new(overlay_creator_state);

        let repl_window_state = ReplWindow::new(repl).unwrap();
        let repl_window =
            ViewStateChannel::<ReplWindow, ()>::new(repl_window_state);

        Self {
            settings,

            fps: Default::default(),
            graph_stats: ViewStateChannel::new(stats),

            node_list,
            node_details,

            path_list,
            path_details,

            overlay_list,
            overlay_creator,

            repl_window,
        }
    }

    pub fn fps(&self) -> &ViewStateChannel<FrameRate, FrameRateMsg> {
        &self.fps
    }

    pub fn graph_stats(&self) -> &ViewStateChannel<GraphStats, GraphStatsMsg> {
        &self.graph_stats
    }

    pub fn node_list(&self) -> &ViewStateChannel<NodeList, NodeListMsg> {
        &self.node_list
    }

    pub fn node_details(
        &self,
    ) -> &ViewStateChannel<NodeDetails, NodeDetailsMsg> {
        &self.node_details
    }

    pub fn apply_received(&mut self) {
        self.fps.apply_received(|state, msg| {
            *state = FrameRate::apply_msg(state, msg);
        });

        self.graph_stats.apply_received(|state, msg| {
            *state = GraphStats::apply_msg(state, msg);
        });

        self.node_list.apply_received(|state, msg| {
            state.apply_msg(msg);
        });

        self.node_details.apply_received(|state, msg| {
            state.apply_msg(msg);
        });

        self.path_list.apply_received(|state, msg| {
            state.apply_msg(msg);
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Views {
    Settings,

    // Console,
    FPS,
    GraphStats,

    NodeList,
    NodeDetails,

    PathList,
    PathDetails,

    ThemeEditor,
    ThemeList,

    OverlayEditor,
    OverlayList,

    EguiInspection,
    EguiSettings,
    EguiMemory,
}

impl Windows {
    pub fn name(&self) -> &str {
        match self {
            Windows::Settings => "Settings",

            Windows::FPS => "FPS",
            Windows::GraphStats => "Graph Stats",

            Windows::Nodes => "Nodes",
            Windows::NodeDetails => "Node Details",

            Windows::Paths => "Paths",

            Windows::Themes => "Themes",
            Windows::Overlays => "Overlays",

            Windows::EguiInspection => "Egui Inspection",
            Windows::EguiSettings => "Egui Settings",
            Windows::EguiMemory => "Egui Memory",
        }
    }

    pub fn views(&self) -> Vec<Views> {
        match self {
            Windows::Settings => vec![Views::Settings],

            Windows::FPS => vec![Views::FPS],
            Windows::GraphStats => vec![Views::GraphStats],

            Windows::Nodes => vec![Views::NodeList],
            Windows::NodeDetails => vec![Views::NodeDetails],

            Windows::Paths => vec![Views::PathList, Views::PathDetails],

            Windows::Themes => vec![Views::ThemeEditor, Views::ThemeList],
            Windows::Overlays => vec![Views::OverlayEditor, Views::OverlayList],

            Windows::EguiInspection => vec![Views::EguiInspection],
            Windows::EguiSettings => vec![Views::EguiSettings],
            Windows::EguiMemory => vec![Views::EguiMemory],
        }
    }

    pub fn all_windows() -> [Windows; 10] {
        [
            Self::Settings,
            Self::FPS,
            Self::GraphStats,
            Self::Nodes,
            Self::Paths,
            Self::Themes,
            Self::Overlays,
            Self::EguiInspection,
            Self::EguiSettings,
            Self::EguiMemory,
        ]
    }
}

impl Views {
    pub fn window(&self) -> Windows {
        match self {
            Self::Settings => Windows::Settings,

            Self::FPS => Windows::FPS,
            Self::GraphStats => Windows::GraphStats,

            Self::NodeList | Views::NodeDetails => Windows::Nodes,
            Self::PathList | Views::PathDetails => Windows::Paths,

            Self::ThemeEditor | Views::ThemeList => Windows::Themes,
            Self::OverlayEditor | Views::OverlayList => Windows::Overlays,

            Self::EguiInspection => Windows::EguiInspection,
            Self::EguiSettings => Windows::EguiSettings,
            Self::EguiMemory => Windows::EguiMemory,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OpenWindows {
    settings: bool,

    fps: bool,
    graph_stats: bool,

    nodes: bool,
    node_details: bool,

    paths: bool,
    path_details: bool,

    themes: bool,
    overlays: bool,
    overlay_creator: bool,

    repl_window: bool,

    egui_inspection: bool,
    egui_settings: bool,
    egui_memory: bool,
}

impl std::default::Default for OpenWindows {
    fn default() -> Self {
        Self {
            settings: false,

            fps: true,
            graph_stats: true,

            nodes: false,
            node_details: false,

            paths: false,
            path_details: false,

            themes: false,
            overlays: false,
            overlay_creator: false,

            repl_window: false,

            egui_inspection: false,
            egui_settings: false,
            egui_memory: false,
        }
    }
}

pub enum GuiMsg {
    EnableView(Views),
    SetWindowOpen { window: Windows, open: Option<bool> },
    SetLightMode,
    SetDarkMode,
    EguiEvent(egui::Event),
    FileDropped { path: std::path::PathBuf },
    Cut,
    Copy,
    Paste,
}

#[derive(Debug, Default, Clone)]
pub struct GuiFocusState {
    mouse_over_gui: Arc<AtomicCell<bool>>,
    wants_keyboard_input: Arc<AtomicCell<bool>>,
    wants_pointer_input: Arc<AtomicCell<bool>>,
}

impl GuiFocusState {
    pub fn mouse_over_gui(&self) -> bool {
        self.mouse_over_gui.load()
    }

    pub fn wants_keyboard_input(&self) -> bool {
        self.wants_keyboard_input.load()
    }

    pub fn wants_pointer_input(&self) -> bool {
        self.wants_pointer_input.load()
    }
}

impl Gui {
    pub fn new(
        app: &GfaestusVk,
        shared_state: SharedState,
        channels: &AppChannels,
        settings: AppSettings,
        graph_query: &GraphQuery,
        thread_pool: Arc<ThreadPool>,
        gff3: Gff3Records,
    ) -> Result<Self> {
        let msaa_samples = app.msaa_samples;
        let render_pass = app.render_passes.gui;

        let draw_system = GuiPipeline::new(app, msaa_samples, render_pass)?;

        let repl = GluonRepl::new(
            channels.app_tx.clone(),
            channels.main_view_tx.clone(),
        )
        .unwrap();

        let ctx = egui::CtxRef::default();

        Self::dark_mode(&ctx);

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

        let hover_node_id: Option<NodeId> = None;

        let windows = Windows::all_windows();

        let windows_active_view = {
            windows
                .iter()
                .copied()
                .filter_map(|w| w.views().first().copied().map(|v| (w, v)))
                .collect::<FxHashMap<_, _>>()
        };

        let open_windows = OpenWindows::default();

        let frame_input = FrameInput::default();

        let app_msg_tx = channels.app_tx.clone();
        let gui_msg_tx = channels.gui_tx.clone();
        let gui_msg_rx = channels.gui_rx.clone();

        let dropped_file = Arc::new(std::sync::Mutex::new(None));

        let view_state = AppViewState::new(
            graph_query,
            &settings,
            &shared_state,
            shared_state.overlay_state().clone(),
            dropped_file.clone(),
            &thread_pool,
            repl,
        );

        let menu_bar = MenuBar::new(shared_state.overlay_state().clone());

        // let annotations = Annotations::from_bed_file(graph, path)

        let clipboard_ctx = ClipboardProvider::new().unwrap();

        let mut path_picker_source = PathPickerSource::new(graph_query)?;

        let gff3_list =
            Gff3RecordList::new(gff3, path_picker_source.create_picker());

        let pwd = std::fs::canonicalize("./")?;
        let mut file_picker =
            FilePicker::new(egui::Id::new("file_picker_test"), &pwd);

        file_picker.goto_dir(&pwd, false);

        let gui = Self {
            ctx,
            frame_input,

            shared_state,
            settings,

            draw_system,

            hover_node_id,

            windows_active_view,

            open_windows,

            view_state,

            gui_msg_tx,
            gui_msg_rx,

            app_msg_tx,

            menu_bar,

            dropped_file,

            thread_pool,

            annotations: Annotations::default(),

            clipboard_ctx,

            gff3_list,

            path_picker_source,

            file_picker,
        };

        Ok(gui)
    }

    fn hover_annotation(&self) {
        if let Some(node_id) = self.hover_node_id {
            if self.ctx.is_pointer_over_area() {
                return;
            }

            let annots = self.annotations.annotations_for(node_id);

            if annots.is_empty() {
                egui::containers::popup::show_tooltip_text(
                    &self.ctx,
                    egui::Id::new("hover_node_id_tooltip"),
                    node_id.0.to_string(),
                )
            } else {
                let mut string = String::new();

                for (name, val) in annots {
                    string.push_str(name);
                    string.push_str(": ");
                    string.push_str(val);
                    string.push_str("\n");
                }

                egui::containers::popup::show_tooltip_text(
                    &self.ctx,
                    egui::Id::new("hover_node_id_tooltip"),
                    string,
                )
            }
        }
    }

    pub fn clone_gui_msg_tx(&self) -> crossbeam::channel::Sender<GuiMsg> {
        self.gui_msg_tx.clone()
    }

    pub fn set_hover_node(&mut self, node: Option<NodeId>) {
        self.hover_node_id = node;
    }

    pub fn app_view_state(&self) -> &AppViewState {
        &self.view_state
    }

    // TODO this should be handled better
    pub fn populate_overlay_list<'a>(
        &mut self,
        names: impl Iterator<Item = (usize, OverlayKind, &'a str)>,
    ) {
        self.view_state.overlay_list.state.populate_names(names);
    }

    pub fn new_overlay_rx(
        &self,
    ) -> &crossbeam::channel::Receiver<OverlayCreatorMsg> {
        &self.view_state.overlay_creator.state.new_overlay_rx()
    }

    pub fn begin_frame(
        &mut self,
        screen_rect: Option<Point>,
        graph_query: &GraphQuery,
        graph_query_worker: &GraphQueryWorker,
        graph_handle: &GraphHandle,
    ) {
        let mut raw_input = self.frame_input.into_raw_input();

        let screen_rect = screen_rect.map(|p| egui::Rect {
            min: Point::ZERO.into(),
            max: p.into(),
        });
        raw_input.screen_rect = screen_rect;

        self.ctx.begin_frame(raw_input);
        {
            let pointer_over_menu_bar =
                if let Some(pos) = self.ctx.input().pointer.hover_pos() {
                    pos.y <= self.menu_bar.height()
                } else {
                    false
                };

            self.shared_state.gui_focus_state.mouse_over_gui.store(
                self.ctx.is_pointer_over_area() || pointer_over_menu_bar,
            );
        }

        self.shared_state
            .gui_focus_state
            .wants_keyboard_input
            .store(self.ctx.wants_keyboard_input());
        self.shared_state
            .gui_focus_state
            .wants_pointer_input
            .store(self.ctx.wants_pointer_input());

        self.menu_bar
            .ui(&self.ctx, &mut self.open_windows, &self.app_msg_tx);

        self.hover_annotation();

        self.view_state.apply_received();

        let scr = self.ctx.input().screen_rect();

        let view_state = &mut self.view_state;

        {
            let overlay_creator = &mut self.open_windows.overlay_creator;
            let overlays = &self.open_windows.overlays;

            if *overlays {
                view_state.overlay_list.state.ui(overlay_creator, &self.ctx);
            }

            view_state.overlay_creator.state.ui(
                overlay_creator,
                graph_handle,
                &self.ctx,
                &self.thread_pool,
            );

            view_state.overlay_list.state.gradient_picker_ui(&self.ctx);
        }

        if let Some(rect) = self.shared_state.active_mouse_rect_screen() {
            let screen_rect = self.ctx.input().screen_rect();

            let paint_area = egui::Ui::new(
                self.ctx.clone(),
                egui::LayerId::new(
                    egui::Order::Background,
                    egui::Id::new("gui_painter_background"),
                ),
                egui::Id::new("gui_painter_ui"),
                screen_rect,
                screen_rect,
            );

            let stroke =
                egui::Stroke::new(2.0, egui::Color32::from_rgb(128, 128, 128));
            paint_area.painter().rect_stroke(rect.into(), 0.0, stroke);
        }

        self.gff3_list.ui(&self.ctx, graph_query, &self.app_msg_tx);

        let mut file_picker_open = true;
        self.file_picker.ui(&self.ctx, &mut file_picker_open);

        if self.open_windows.settings {
            view_state.settings.ui(&self.ctx);
        }

        if self.open_windows.fps {
            let top = self.menu_bar.height();

            view_state.fps.state.ui(
                &self.ctx,
                Point {
                    x: scr.max.x - 100.0,
                    y: top,
                },
                None,
            );
        }

        if self.open_windows.graph_stats {
            let top = self.menu_bar.height();

            view_state.graph_stats.state.ui(
                &self.ctx,
                Point { x: 0.0, y: top },
                None,
            );
        }

        {
            let node_list = &self.open_windows.nodes;
            let node_details = &mut self.open_windows.node_details;

            let path_details = &mut self.open_windows.path_details;
            let path_details_id_cell =
                view_state.path_details.state.path_details.path_id_cell();

            if *node_list {
                view_state.node_list.state.ui(
                    &self.ctx,
                    &self.app_msg_tx,
                    node_details,
                    graph_query,
                );
            }

            if *node_details {
                view_state.node_details.state.ui(
                    node_details,
                    graph_query,
                    &self.ctx,
                    path_details_id_cell,
                    path_details,
                );
            }
        }

        {
            let path_list = &self.open_windows.paths;
            let path_details = &mut self.open_windows.path_details;

            let node_details = &mut self.open_windows.node_details;
            let node_details_id_cell =
                view_state.node_details.state.node_id_cell();

            if *path_list {
                view_state.path_list.state.ui(
                    &self.ctx,
                    &self.app_msg_tx,
                    path_details,
                    graph_query,
                );
            }

            if *path_details {
                view_state.path_details.state.ui(
                    path_details,
                    graph_query,
                    graph_query_worker,
                    &self.ctx,
                    node_details_id_cell,
                    node_details,
                    &self.app_msg_tx,
                );
            }
        }

        if self.open_windows.repl_window {
            let repl_window = &mut self.open_windows.repl_window;
            view_state.repl_window.state.ui(
                repl_window,
                &self.ctx,
                &self.thread_pool,
            );
        }

        if self.open_windows.egui_inspection {
            egui::Window::new("egui_inspection_ui_window")
                .show(&self.ctx, |ui| self.ctx.inspection_ui(ui));
        }

        if self.open_windows.egui_settings {
            egui::Window::new("egui_settings_ui_window")
                .show(&self.ctx, |ui| self.ctx.settings_ui(ui));
        }

        if self.open_windows.egui_memory {
            egui::Window::new("egui_memory_ui_window")
                .show(&self.ctx, |ui| self.ctx.memory_ui(ui));
        }
    }

    pub fn end_frame(&mut self) -> Vec<egui::ClippedMesh> {
        let (output, shapes) = self.ctx.end_frame();

        if !output.copied_text.is_empty() {
            self.clipboard_ctx.set_contents(output.copied_text).unwrap();
        }

        self.ctx.tessellate(shapes)
    }

    pub fn active_views(&self) -> Vec<Views> {
        let mut views: Vec<_> =
            self.windows_active_view.values().copied().collect();
        views.sort();
        views
    }

    pub fn pointer_over_gui(&self) -> bool {
        self.ctx.is_pointer_over_area()
    }

    pub fn upload_texture(&mut self, app: &GfaestusVk) -> Result<()> {
        let egui_tex = self.ctx.texture();
        if egui_tex.version != self.draw_system.texture_version() {
            self.draw_system.upload_texture(
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
        self.draw_system.vertices.upload_meshes(app, meshes)
    }

    pub fn draw(
        &self,
        cmd_buf: vk::CommandBuffer,
        render_pass: vk::RenderPass,
        framebuffers: &Framebuffers,
        screen_dims: [f32; 2],
        push_descriptor: &PushDescriptor,
        gradients: &Gradients,
    ) -> Result<()> {
        self.draw_system.draw(
            cmd_buf,
            render_pass,
            framebuffers,
            screen_dims,
            push_descriptor,
            gradients,
        )
    }

    pub fn push_event(&mut self, event: egui::Event) {
        self.frame_input.events.push(event);
    }

    pub fn apply_received_gui_msgs(&mut self) {
        while let Ok(msg) = self.gui_msg_rx.try_recv() {
            match msg {
                GuiMsg::EnableView(view) => {
                    //
                }
                GuiMsg::SetWindowOpen { window, open } => {
                    let open_windows = &mut self.open_windows;

                    let win_state = match window {
                        Windows::Settings => &mut open_windows.settings,
                        Windows::FPS => &mut open_windows.fps,
                        Windows::GraphStats => &mut open_windows.graph_stats,
                        Windows::Nodes => &mut open_windows.nodes,
                        Windows::NodeDetails => &mut open_windows.node_details,
                        Windows::Paths => &mut open_windows.paths,
                        Windows::Themes => &mut open_windows.themes,
                        Windows::Overlays => &mut open_windows.overlays,
                        Windows::EguiInspection => {
                            &mut open_windows.egui_inspection
                        }
                        Windows::EguiSettings => {
                            &mut open_windows.egui_settings
                        }
                        Windows::EguiMemory => &mut open_windows.egui_memory,
                    };

                    if let Some(open) = open {
                        *win_state = open;
                    } else {
                        *win_state = !*win_state;
                    }
                }
                GuiMsg::SetLightMode => {
                    Self::light_mode(&self.ctx);
                }
                GuiMsg::SetDarkMode => {
                    Self::dark_mode(&self.ctx);
                }
                GuiMsg::EguiEvent(event) => {
                    self.frame_input.events.push(event);
                }
                GuiMsg::FileDropped { path } => {
                    if let Ok(mut guard) = self.dropped_file.lock() {
                        println!(
                            "Updated dropped file with {:?}",
                            path.to_str()
                        );
                        *guard = Some(path);
                    }
                }
                GuiMsg::Cut => {
                    self.frame_input.events.push(egui::Event::Cut);
                }
                GuiMsg::Copy => {
                    self.frame_input.events.push(egui::Event::Copy);
                }
                GuiMsg::Paste => {
                    if let Ok(text) = &self.clipboard_ctx.get_contents() {
                        self.frame_input
                            .events
                            .push(egui::Event::Text(text.clone()));
                    }
                }
            }
        }
    }

    pub fn apply_input(
        &mut self,
        app_msg_tx: &crossbeam::channel::Sender<crate::app::AppMsg>,
        input: SystemInput<GuiInput>,
    ) {
        use GuiInput as In;
        let payload = input.payload();

        match input {
            SystemInput::Keyboard { state, .. } => {
                if state.pressed() {
                    match payload {
                        GuiInput::KeyEguiInspectionUi => {
                            self.gui_msg_tx
                                .send(GuiMsg::SetWindowOpen {
                                    window: Windows::EguiInspection,
                                    open: None,
                                })
                                .unwrap();
                        }
                        GuiInput::KeyEguiSettingsUi => {
                            self.gui_msg_tx
                                .send(GuiMsg::SetWindowOpen {
                                    window: Windows::EguiSettings,
                                    open: None,
                                })
                                .unwrap();
                        }
                        GuiInput::KeyEguiMemoryUi => {
                            self.gui_msg_tx
                                .send(GuiMsg::SetWindowOpen {
                                    window: Windows::EguiMemory,
                                    open: None,
                                })
                                .unwrap();
                        }
                        // GuiInput::KeyToggleRender(opt) => {
                        //     use crate::app::AppConfigMsg as Msg;
                        //     use crate::app::RenderConfigOpts as Opts;

                        //     let cfg_msg = match opt {
                        //         Opts::SelOutlineEdge => {
                        //             Msg::ToggleSelectionEdgeDetect
                        //         }
                        //         Opts::SelOutlineBlur => {
                        //             Msg::ToggleSelectionEdgeBlur
                        //         }
                        //         Opts::SelOutline => Msg::ToggleSelectionOutline,
                        //         Opts::NodesColor => Msg::ToggleNodesColor,
                        //     };

                        //     cfg_msg_tx.send(cfg_msg).unwrap();
                        // }
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
                    let mut delta = delta;
                    if delta.abs() < 4.0 {
                        delta = delta.signum() * 4.0;
                    }

                    self.frame_input.scroll_delta += delta;
                }
            }
        }
    }

    fn set_style(ctx: &egui::CtxRef, visuals: egui::style::Visuals) {
        let mut style: egui::Style = (*ctx.style()).clone();
        style.visuals = visuals;
        style.visuals.window_corner_radius = 0.0;
        style.visuals.window_shadow.extrusion = 0.0;
        ctx.set_style(style);
    }

    fn light_mode(ctx: &egui::CtxRef) {
        Self::set_style(ctx, egui::style::Visuals::light());
    }

    fn dark_mode(ctx: &egui::CtxRef) {
        Self::set_style(ctx, egui::style::Visuals::dark());
    }
}

/// Wrapper for input events that are fed into egui
#[derive(Debug, Default, Clone)]
struct FrameInput {
    events: Vec<egui::Event>,
    scroll_delta: f32,
}

impl FrameInput {
    fn into_raw_input(&mut self) -> egui::RawInput {
        let mut raw_input = egui::RawInput::default();
        // TODO maybe use clone_from and clear self.events instead, to reduce allocations
        raw_input.events = std::mem::take(&mut self.events);
        raw_input.scroll_delta = egui::Vec2 {
            x: 0.0,
            y: self.scroll_delta,
        };
        self.scroll_delta = 0.0;

        raw_input
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
    // KeyToggleRender(RenderConfigOpts),
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
            // (
            //     Key::Key1,
            //     Input::KeyToggleRender(RenderConfigOpts::SelOutlineEdge),
            // ),
            // (
            //     Key::Key2,
            //     Input::KeyToggleRender(RenderConfigOpts::SelOutlineBlur),
            // ),
            // (
            //     Key::Key3,
            //     Input::KeyToggleRender(RenderConfigOpts::SelOutline),
            // ),
            // (
            //     Key::Key4,
            //     Input::KeyToggleRender(RenderConfigOpts::NodesColor),
            // ),
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
