use std::{path::PathBuf, sync::Arc};

use clipboard::{ClipboardContext, ClipboardProvider};

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

use crossbeam::atomic::AtomicCell;

use crate::{
    annotations::{
        AnnotationFileType, Annotations, BedColumn, BedRecords, Gff3Column,
        Gff3Records, Labels,
    },
    app::{
        App, AppChannels, AppMsg, AppSettings, OverlayCreatorMsg, SharedState,
    },
    context::ContextMgr,
    reactor::Reactor,
    universe::Node,
    vulkan::compute::path_view::PathViewRenderer,
    vulkan::{render_pass::Framebuffers, texture::Gradients},
    window::{GuiChannels, GuiId, GuiWindows},
};
use crate::{app::OverlayState, geometry::*};

use crate::overlays::OverlayKind;

use crate::graph_query::GraphQuery;

use crate::input::binds::{
    BindableInput, KeyBind, MouseButtonBind, SystemInput, SystemInputBindings,
    WheelBind,
};

use crate::vulkan::{draw_system::gui::GuiPipeline, GfaestusVk};

use ash::vk;

#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

pub mod console;
pub mod debug;
pub mod text;
pub mod util;
pub mod widgets;
pub mod windows;

use console::*;
use debug::*;
#[allow(unused_imports)]
use util::*;
use widgets::*;
use windows::*;

use self::text::draw_text_at_world_point;

pub struct Gui {
    pub ctx: egui::CtxRef,
    frame_input: FrameInput,

    shared_state: SharedState,
    channels: AppChannels,
    #[allow(dead_code)]
    settings: AppSettings,

    pub draw_system: GuiPipeline,

    open_windows: OpenWindows,
    view_state: AppViewState,

    menu_bar: MenuBar,

    dropped_file: Arc<std::sync::Mutex<Option<PathBuf>>>,

    gff3_list: RecordList<Gff3Records>,
    bed_list: RecordList<BedRecords>,

    annotation_file_list: AnnotationFileList,

    pub console: Console<'static>,
    console_down: bool,

    windows: GuiWindows,
    gui_channels: GuiChannels,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Windows {
    Settings,

    AnnotationRecords,

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
    settings: SettingsWindow,
    fps: ViewStateChannel<FrameRate, FrameRateMsg>,

    graph_stats: ViewStateChannel<GraphStats, GraphStatsMsg>,

    node_list: ViewStateChannel<NodeList, NodeListMsg>,
    node_details: ViewStateChannel<NodeDetails, NodeDetailsMsg>,

    path_list: ViewStateChannel<PathList, ()>,
    path_details: ViewStateChannel<PathDetails, ()>,

    // theme_editor: ThemeEditor,
    // theme_list: ThemeList,
    overlay_creator: ViewStateChannel<OverlayCreator, OverlayCreatorMsg>,
    overlay_list: ViewStateChannel<OverlayList, OverlayListMsg>,
}

impl AppViewState {
    pub fn new(
        reactor: &Reactor,
        settings: &AppSettings,
        shared_state: &SharedState,
        overlay_state: OverlayState,
        _dropped_file: Arc<std::sync::Mutex<Option<PathBuf>>>,
    ) -> Self {
        let graph_query = reactor.graph_query.clone();
        let graph = graph_query.graph();

        let stats = GraphStats {
            node_count: graph.node_count(),
            edge_count: graph.edge_count(),
            path_count: graph.path_count(),
            total_len: graph.total_length(),
        };

        let settings = SettingsWindow::new(settings, shared_state);

        let node_details_state = NodeDetails::default();
        let node_id_cell = node_details_state.node_id_cell().clone();
        let node_details = ViewStateChannel::<NodeDetails, NodeDetailsMsg>::new(
            node_details_state,
        );

        let node_list_state = NodeList::new(&graph_query, node_id_cell.clone());
        let node_list =
            ViewStateChannel::<NodeList, NodeListMsg>::new(node_list_state);

        let path_details_state = PathDetails::new(reactor);
        let path_id_cell =
            path_details_state.path_details.path_id_cell().clone();
        let path_details =
            ViewStateChannel::<PathDetails, ()>::new(path_details_state);

        let path_list_state = PathList::new(&graph_query, path_id_cell);
        let path_list = ViewStateChannel::<PathList, ()>::new(path_list_state);

        let overlay_list_state = OverlayList::new(overlay_state);
        let overlay_list = ViewStateChannel::<OverlayList, OverlayListMsg>::new(
            overlay_list_state,
        );

        let overlay_creator_state = OverlayCreator::new(reactor).unwrap();
        let overlay_creator = ViewStateChannel::<
            OverlayCreator,
            OverlayCreatorMsg,
        >::new(overlay_creator_state);

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
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OpenWindows {
    settings: bool,

    annotation_files: bool,
    annotation_records: bool,
    label_set_list: bool,

    nodes: bool,
    node_details: bool,

    paths: bool,
    path_details: bool,

    themes: bool,
    overlays: bool,
    overlay_creator: bool,
}

impl std::default::Default for OpenWindows {
    fn default() -> Self {
        Self {
            settings: false,

            annotation_files: false,
            annotation_records: false,
            label_set_list: false,

            nodes: false,
            node_details: false,

            paths: false,
            path_details: false,

            themes: false,
            overlays: false,
            overlay_creator: false,
        }
    }
}

pub enum GuiMsg {
    SetWindowOpen { window: Windows, open: Option<bool> },
    SetLightMode,
    SetDarkMode,

    EguiEvent(egui::Event),
    FileDropped { path: std::path::PathBuf },

    Cut,
    Copy,
    Paste,

    // TODO this shouldn't really be here, as things like the console
    // will never update the modifiers
    SetModifiers(winit::event::ModifiersState),
}

// TODO: this can probably be replaced by egui's built in focus tracking
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
        app: &App,
        gfaestus: &GfaestusVk,
        path_view_renderer: &Arc<PathViewRenderer>,
    ) -> Result<Self> {
        let reactor = &app.reactor;
        let channels = app.channels();
        let shared_state = app.shared_state().clone();
        let settings = app.settings.clone();

        let graph_query = reactor.graph_query.clone();

        let render_pass = gfaestus.render_passes.gui;

        let draw_system = GuiPipeline::new(gfaestus, render_pass)?;

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

        let open_windows = OpenWindows::default();

        let frame_input = FrameInput::default();

        let dropped_file = Arc::new(std::sync::Mutex::new(None));

        let view_state = AppViewState::new(
            reactor,
            &settings,
            &shared_state,
            shared_state.overlay_state().clone(),
            dropped_file.clone(),
        );

        let menu_bar = MenuBar::new(shared_state.overlay_state().clone());

        // let clipboard_ctx = ClipboardProvider::new().unwrap();

        let mut path_picker_source = PathPickerSource::new(&graph_query)?;

        let annotation_file_list = AnnotationFileList::new(
            reactor,
            channels.app_tx.clone(),
            channels.gui_tx.clone(),
        )?;

        let console = Console::new(
            reactor,
            channels.clone(),
            settings.to_owned(),
            shared_state.to_owned(),
        );

        let gff3_list = {
            let mut list = RecordList::new(
                reactor,
                egui::Id::new("gff3_records_list"),
                path_picker_source.create_picker(),
            );

            list.add_scroll_console_setter(&console, "gff3_records_scroll_ix");

            use Gff3Column as Gff;

            list.set_default_columns(
                [Gff::Source, Gff::Type, Gff::Frame],
                [Gff::SeqId, Gff::Start, Gff::End, Gff::Strand],
            );

            list
        };

        let bed_list = {
            let mut list = RecordList::new(
                reactor,
                egui::Id::new("bed_records_list"),
                path_picker_source.create_picker(),
            );

            list.add_scroll_console_setter(&console, "bed_records_scroll_ix");

            use BedColumn as Bed;

            list.set_default_columns([], [Bed::Chr, Bed::Start, Bed::End]);

            list
        };

        let mut windows = GuiWindows::default();

        {
            let path_view_id = egui::Id::new("path_view_window");
            let gui_id = GuiId::new(path_view_id);

            let mut path_view_state =
                PathPositionList::new(path_view_renderer.clone());

            windows.add_window(
                gui_id,
                "Path View",
                move |app: &App, ui: &mut egui::Ui, nodes: &[Node]| {
                    let App {
                        reactor,
                        channels,
                        shared_state,
                        ..
                    } = app;

                    path_view_state.ui_impl(
                        ui,
                        reactor,
                        channels,
                        shared_state,
                        nodes,
                    );
                },
            );
        }

        {
            /*
            let annotation_file_list = AnnotationFileList::new(
                reactor,
                channels.app_tx.clone(),
                channels.gui_tx.clone(),
            )?;

            let afl_id = GuiId::new("_annotation_file_list");

            windows.add_window(afl_id, "Annotation Files", |ui| {
                todo!();
            });

            let cur_annot = annotation_file_list.current_annotation.clone();
            */

            /*
            let gff3_list = {
                let mut list = RecordList::new(
                    reactor,
                    egui::Id::new("gff3_records_list"),
                    path_picker_source.create_picker(),
                );

                use Gff3Column as Gff;

                list.set_default_columns(
                    [Gff::Source, Gff::Type, Gff::Frame],
                    [Gff::SeqId, Gff::Start, Gff::End, Gff::Strand],
                );

                list
            };

            let bed_list = {
                let mut list = RecordList::new(
                    reactor,
                    egui::Id::new("bed_records_list"),
                    path_picker_source.create_picker(),
                );

                use BedColumn as Bed;

                list.set_default_columns([], [Bed::Chr, Bed::Start, Bed::End]);

                list
            };


            let gff3_window = || {
                egui::Window::new("GFF3")
                    .default_pos(egui::Pos2::new(600.0, 200.0))
                    .collapsible(true)
            };

            let bed_window = || {
                egui::Window::new("BED")
                    .default_pos(egui::Pos2::new(600.0, 200.0))
                    .collapsible(true)
            };
            */

            // let show_
        }

        // windows.

        let gui = Self {
            ctx,
            frame_input,

            shared_state: shared_state.clone(),
            channels: channels.clone(),
            settings: settings.clone(),

            draw_system,

            open_windows,

            view_state,

            menu_bar,

            dropped_file,

            // clipboard_ctx,
            gff3_list,
            bed_list,

            annotation_file_list,

            console_down: false,
            console,

            windows,
            gui_channels: GuiChannels::new(),
        };

        Ok(gui)
    }

    pub fn app_view_state(&self) -> &AppViewState {
        &self.view_state
    }

    // TODO this should be handled better
    pub fn populate_overlay_list<'a>(
        &mut self,
        // TODO should be a slice, but this function shouldn't exist, so
        names: impl Iterator<Item = (usize, OverlayKind, &'a str)>,
    ) {
        let names = names.collect::<Vec<_>>();

        self.view_state
            .overlay_list
            .state
            .populate_names(names.iter().copied());

        self.console.populate_overlay_list(&names);

        self.menu_bar.populate_overlay_list(
            &self.view_state.overlay_list.state.overlay_names,
        );
    }

    pub fn scroll_to_gff_record(
        &mut self,
        records: &Gff3Records,
        column: &Gff3Column,
        value: &[u8],
    ) {
        self.gff3_list
            .scroll_to_label_record(records, column, value);
    }

    pub fn scroll_to_bed_record(
        &mut self,
        records: &BedRecords,
        column: &BedColumn,
        value: &[u8],
    ) {
        self.bed_list.scroll_to_label_record(records, column, value);
    }

    pub fn begin_frame(
        &mut self,
        app: &App,
        // ctx_tx: &crossbeam::channel::Sender<ContextEntry>,
        ctx_mgr: &ContextMgr,
        nodes: &[Node],
    ) {
        let App {
            reactor,
            annotations,
            labels,
            ..
        } = app;

        let graph_query = reactor.graph_query.as_ref();

        let new_screen_rect: Option<Point> = Some(app.dims().into());

        let mut raw_input = self.frame_input.into_raw_input();

        let screen_rect = new_screen_rect.map(|p| egui::Rect {
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

        self.menu_bar.ui(
            &self.ctx,
            &mut self.open_windows,
            &self.channels.app_tx,
            &self.windows,
        );

        self.console.ui(&self.ctx, self.console_down, reactor);

        self.view_state.apply_received();

        let scr = self.ctx.input().screen_rect();

        let view_state = &mut self.view_state;

        {
            let overlay_creator = &mut self.open_windows.overlay_creator;
            let overlays = &mut self.open_windows.overlays;

            view_state.overlay_list.state.ui(
                &self.ctx,
                overlays,
                overlay_creator,
            );

            view_state
                .overlay_creator
                .state
                .ui(&self.ctx, overlay_creator);

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

        self.annotation_file_list.ui(
            &self.ctx,
            &mut self.open_windows.annotation_files,
            &self.channels.gui_tx,
            annotations,
        );

        {
            let path_view_id = egui::Id::new("path_view_window");
            let gui_id = GuiId::new(path_view_id);

            let open = self.windows.get_open_arc(gui_id).unwrap();
            let mut is_open = open.load();

            let window = egui::Window::new("Path View")
                .id(path_view_id)
                .open(&mut is_open);

            self.windows
                .show_in_window(&app, &self.ctx, nodes, gui_id, window);

            open.store(is_open);
        }

        {
            let read = self.annotation_file_list.current_annotation();
            if let Some((annot_type, annot_name)) = read.as_ref() {
                match annot_type {
                    AnnotationFileType::Gff3 => {
                        if let Some(records) = annotations.get_gff3(annot_name)
                        {
                            let ctx = &self.ctx;
                            let open =
                                &mut self.open_windows.annotation_records;
                            let app_msg_tx = &self.channels.app_tx;

                            let gff3_list = &mut self.gff3_list;

                            let _resp = egui::Window::new("GFF3")
                                .default_pos(egui::Pos2::new(600.0, 200.0))
                                .collapsible(true)
                                .open(open)
                                .show(ctx, |ui| {
                                    gff3_list.ui(
                                        ui,
                                        graph_query,
                                        app_msg_tx,
                                        annot_name,
                                        records,
                                    )
                                });
                        }
                    }
                    AnnotationFileType::Bed => {
                        if let Some(records) = annotations.get_bed(annot_name) {
                            let ctx = &self.ctx;
                            let open =
                                &mut self.open_windows.annotation_records;
                            let app_msg_tx = &self.channels.app_tx;

                            let bed_list = &mut self.bed_list;

                            let _resp = egui::Window::new("BED")
                                .default_pos(egui::Pos2::new(600.0, 200.0))
                                .collapsible(true)
                                .open(open)
                                .show(ctx, |ui| {
                                    bed_list.ui(
                                        ui,
                                        graph_query,
                                        app_msg_tx,
                                        annot_name,
                                        records,
                                    )
                                });
                        }
                    }
                }
            }
        }

        LabelSetList::ui(
            // &self.channels,
            // &self.shared_state,
            &self.ctx,
            &mut self.open_windows.label_set_list,
            labels,
        );

        view_state
            .settings
            .ui(&self.ctx, &mut self.open_windows.settings);

        if view_state.settings.gui.show_fps {
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

        if view_state.settings.gui.show_graph_stats {
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
                    &self.channels.app_tx,
                    node_details,
                    graph_query,
                    ctx_mgr,
                );
            }

            if *node_details {
                view_state.node_details.state.ui(
                    node_details,
                    graph_query,
                    &self.ctx,
                    path_details_id_cell,
                    path_details,
                    ctx_mgr,
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
                    &self.channels.app_tx,
                    path_details,
                    graph_query,
                    ctx_mgr,
                );
            }

            if *path_details {
                view_state.path_details.state.ui(
                    path_details,
                    graph_query,
                    &self.ctx,
                    node_details_id_cell,
                    node_details,
                    &self.channels.app_tx,
                    ctx_mgr,
                );
            }
        }

        {
            let debug = &mut view_state.settings.debug;
            let inspection = &mut debug.egui_inspection;
            let settings = &mut debug.egui_settings;
            let memory = &mut debug.egui_memory;

            let ctx = &self.ctx;

            egui::Window::new("egui_inspection_ui_window")
                .open(inspection)
                .show(ctx, |ui| ctx.inspection_ui(ui));

            egui::Window::new("egui_settings_ui_window")
                .open(settings)
                .show(ctx, |ui| ctx.settings_ui(ui));

            egui::Window::new("egui_memory_ui_window")
                .open(memory)
                .show(ctx, |ui| ctx.memory_ui(ui));
        }

        let settings = &self.app_view_state().settings;

        if settings.debug.view_info {
            let view = self.shared_state.view();
            ViewDebugInfo::ui(&self.ctx, view);
        }

        if settings.debug.cursor_info {
            let view = self.shared_state.view();
            let mouse = self.shared_state.mouse_pos();
            MouseDebugInfo::ui(&self.ctx, view, mouse);
        }
    }

    pub fn end_frame(
        &mut self,
        reactor: &mut Reactor,
    ) -> Vec<egui::ClippedMesh> {
        let (output, shapes) = self.ctx.end_frame();

        if !output.copied_text.is_empty() {
            reactor.set_clipboard_contents(&output.copied_text, true);
        }

        self.ctx.tessellate(shapes)
    }

    pub fn pointer_over_gui(&self) -> bool {
        self.ctx.is_pointer_over_area()
    }

    pub fn upload_egui_texture(&mut self, app: &GfaestusVk) -> Result<()> {
        log::trace!("Gui::upload_texture");

        let egui_tex = self.ctx.texture();
        if egui_tex.version != self.draw_system.egui_texture_version() {
            log::trace!(
                "Texture version difference, uploading new GUI texture"
            );
            self.draw_system.upload_egui_texture(
                app,
                app.transient_command_pool,
                app.graphics_queue,
                &egui_tex,
            )?;
            log::trace!("Texture upload complete");
        }

        Ok(())
    }

    pub fn upload_vertices(
        &mut self,
        app: &GfaestusVk,
        meshes: &[egui::ClippedMesh],
    ) -> Result<()> {
        log::trace!("Uploading GUI vertices");
        self.draw_system.vertices.upload_meshes(app, meshes)
    }

    pub fn draw(
        &self,
        cmd_buf: vk::CommandBuffer,
        render_pass: vk::RenderPass,
        framebuffers: &Framebuffers,
        screen_dims: [f32; 2],
    ) -> Result<()> {
        self.draw_system
            .draw(cmd_buf, render_pass, framebuffers, screen_dims)
    }

    pub fn push_event(&mut self, event: egui::Event) {
        self.frame_input.events.push(event);
    }

    pub fn apply_received_gui_msgs(&mut self, reactor: &mut Reactor) {
        while let Ok(msg) = self.channels.gui_rx.try_recv() {
            match msg {
                GuiMsg::SetWindowOpen { window, open } => {
                    let open_windows = &mut self.open_windows;
                    let view_state = &mut self.view_state;

                    let win_state = match window {
                        Windows::Settings => &mut open_windows.settings,
                        Windows::AnnotationRecords => {
                            &mut open_windows.annotation_records
                        }
                        Windows::Nodes => &mut open_windows.nodes,
                        Windows::NodeDetails => &mut open_windows.node_details,
                        Windows::Paths => &mut open_windows.paths,
                        Windows::Themes => &mut open_windows.themes,
                        Windows::Overlays => &mut open_windows.overlays,
                        Windows::EguiInspection => {
                            &mut view_state.settings.debug.egui_inspection
                        }
                        Windows::EguiSettings => {
                            &mut view_state.settings.debug.egui_settings
                        }
                        Windows::EguiMemory => {
                            &mut view_state.settings.debug.egui_memory
                        }
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
                        trace!("Updated dropped file with {:?}", path.to_str());
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
                    if let Some(text) = reactor.get_clipboard_contents(true) {
                        self.frame_input.events.push(egui::Event::Text(text));
                    }
                }
                GuiMsg::SetModifiers(mods) => {
                    let modifiers = egui::Modifiers {
                        alt: mods.alt(),
                        ctrl: mods.ctrl(),
                        shift: mods.shift(),
                        mac_cmd: mods.logo(),
                        command: mods.logo(),
                    };

                    self.frame_input.modifiers = modifiers;
                }
            }
        }
    }

    pub fn apply_input(
        &mut self,
        _app_msg_tx: &crossbeam::channel::Sender<crate::app::AppMsg>,
        input: SystemInput<GuiInput>,
    ) {
        use GuiInput as In;
        let payload = input.payload();

        match input {
            SystemInput::Keyboard { state, .. } => {
                if state.pressed() {
                    match payload {
                        GuiInput::KeyEguiInspectionUi => {
                            self.channels
                                .gui_tx
                                .send(GuiMsg::SetWindowOpen {
                                    window: Windows::EguiInspection,
                                    open: None,
                                })
                                .unwrap();
                        }
                        GuiInput::KeyEguiSettingsUi => {
                            self.channels
                                .gui_tx
                                .send(GuiMsg::SetWindowOpen {
                                    window: Windows::EguiSettings,
                                    open: None,
                                })
                                .unwrap();
                        }
                        GuiInput::KeyEguiMemoryUi => {
                            self.channels
                                .gui_tx
                                .send(GuiMsg::SetWindowOpen {
                                    window: Windows::EguiMemory,
                                    open: None,
                                })
                                .unwrap();
                        }
                        GuiInput::KeyToggleConsole => {
                            self.console_down = !self.console_down;
                            if self.console_down {
                                self.ctx.memory().request_focus(egui::Id::new(
                                    console::Console::ID_TEXT,
                                ));
                            }
                        }
                        GuiInput::KeyConsoleDown => {
                            self.console_down = true;
                            self.ctx.memory().request_focus(egui::Id::new(
                                console::Console::ID_TEXT,
                            ));
                        }
                        GuiInput::KeyConsoleUp => {
                            self.console_down = false;
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
    modifiers: egui::Modifiers,
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
        raw_input.modifiers = self.modifiers;
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
    KeyToggleConsole,
    KeyConsoleDown,
    KeyConsoleUp,
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
            (Key::Escape, Input::KeyConsoleUp),
            (Key::Grave, Input::KeyConsoleDown),
            (Key::F4, Input::KeyToggleConsole),
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
