use crossbeam::{atomic::AtomicCell, channel::Sender};
#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};
use rustc_hash::FxHashMap;

use crate::{
    app::AppMsg,
    overlays::OverlayKind,
    window::{GuiId, GuiWindows},
};
use crate::{app::OverlayState, geometry::*};

pub trait Widget {
    fn id() -> &'static str;

    fn ui(
        &self,
        ctx: &egui::CtxRef,
        pos: Point,
        size: Option<Point>,
    ) -> Option<egui::InnerResponse<Option<()>>>;
}

pub struct MenuBar {
    overlay_state: OverlayState,

    overlay_list: Vec<(usize, String)>,

    height: AtomicCell<f32>,
}

impl MenuBar {
    pub const ID: &'static str = "app_menu_bar";

    pub fn new(overlay_state: OverlayState) -> Self {
        Self {
            overlay_state,
            overlay_list: Vec::new(),
            height: AtomicCell::new(0.0),
        }
    }

    pub fn height(&self) -> f32 {
        self.height.load()
    }

    pub fn populate_overlay_list(
        &mut self,
        names: &FxHashMap<usize, (OverlayKind, String)>,
    ) {
        let mut overlay_list = names
            .iter()
            .map(|(ix, (_, name))| (*ix, name.to_owned()))
            .collect::<Vec<_>>();
        overlay_list.sort_by_key(|(ix, _)| *ix);
        self.overlay_list = overlay_list;
    }

    pub fn ui<'a>(
        &self,
        ctx: &egui::CtxRef,
        open_windows: &'a mut super::OpenWindows,
        app_msg_tx: &Sender<AppMsg>,
        windows: &GuiWindows,
    ) {
        let settings = &mut open_windows.settings;

        let annotation_records = &mut open_windows.annotation_records;
        let annotation_files = &mut open_windows.annotation_files;
        let label_set_list = &mut open_windows.label_set_list;

        let nodes = &mut open_windows.nodes;
        let paths = &mut open_windows.paths;

        // let path_view = &mut open_windows.path_position_list;

        let _themes = &mut open_windows.themes;
        let overlays = &mut open_windows.overlays;

        let resp = egui::TopBottomPanel::top(Self::ID).show(ctx, |ui| {
            use egui::menu;

            menu::bar(ui, |ui| {
                menu::menu(ui, "Graph", |ui| {
                    if ui.selectable_label(*nodes, "Nodes").clicked() {
                        *nodes = !*nodes;
                    }

                    if ui.selectable_label(*paths, "Paths").clicked() {
                        *paths = !*paths;
                    }

                    ui.separator();

                    let path_view_id = egui::Id::new("path_view_window");
                    let gui_id = GuiId::new(path_view_id);

                    let path_view = windows.is_open(gui_id);

                    if ui.selectable_label(path_view, "Path View").clicked() {
                        windows.set_open(gui_id, !path_view);
                        // windows.toggle_open(gui_id);
                        // *path_view = !*path_view;
                    }
                });

                menu::menu(ui, "Annotations", |ui| {
                    if ui.selectable_label(*annotation_files, "Files").clicked()
                    {
                        *annotation_files = !*annotation_files;
                    }

                    if ui
                        .selectable_label(*annotation_records, "Records")
                        .clicked()
                    {
                        *annotation_records = !*annotation_records;
                    }

                    if ui
                        .selectable_label(*label_set_list, "Label sets")
                        .clicked()
                    {
                        *label_set_list = !*label_set_list;
                    }
                });

                menu::menu(ui, "Overlays", |ui| {
                    if ui.selectable_label(*overlays, "Overlay list").clicked()
                    {
                        *overlays = !*overlays;
                    }
                });

                menu::menu(ui, "View", |ui| {
                    if ui.button("Goto selection").clicked() {
                        app_msg_tx.send(AppMsg::goto_selection()).unwrap();
                    }
                });

                menu::menu(ui, "Tools", |ui| {
                    if ui.selectable_label(*settings, "Settings").clicked() {
                        *settings = !*settings;
                    }

                    ui.separator();

                    if ui.button("BED Label Wizard").clicked() {
                        let script = "bed_label_wizard()".to_string();
                        app_msg_tx
                            .send(AppMsg::ConsoleEval { script })
                            .unwrap();
                    }

                    if ui.button("TSV Import").clicked() {
                        let script = "tsv_wizard()".to_string();
                        app_msg_tx
                            .send(AppMsg::ConsoleEval { script })
                            .unwrap();
                    }
                });

                let mut selected =
                    self.overlay_state.current_overlay().unwrap();
                let overlay_count = self.overlay_list.len();

                ui.separator();

                let overlay_list =
                    egui::ComboBox::from_id_source("menu_bar_overlay_list")
                        .show_index(
                            ui,
                            &mut selected,
                            overlay_count,
                            |ix: usize| {
                                let (_, name) =
                                    self.overlay_list.get(ix).unwrap();
                                name.to_string()
                            },
                        );

                if overlay_list.changed() {
                    self.overlay_state.set_current_overlay(Some(selected));
                }
            });
        });

        let height = resp.response.rect.height();
        self.height.store(height);
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NodeInfo {
    node_id: NodeId,
    len: usize,
    degree: (usize, usize),
    coverage: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FrameRate {
    pub fps: f32,
    pub frame_time: f32,
    pub frame: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FrameRateMsg(pub FrameRate);

impl FrameRate {
    pub fn apply_msg(&self, msg: FrameRateMsg) -> Self {
        msg.0
    }
}

impl Widget for FrameRate {
    #[inline]
    fn id() -> &'static str {
        "frame_rate_box"
    }

    fn ui(
        &self,
        ctx: &egui::CtxRef,
        pos: Point,
        _size: Option<Point>,
    ) -> Option<egui::InnerResponse<Option<()>>> {
        let scr = ctx.input().screen_rect();

        let width = 100.0;

        let rect = egui::Rect {
            min: pos.into(),
            max: Point {
                x: scr.max.x,
                y: pos.y + 100.0,
            }
            .into(),
        };

        egui::Window::new(Self::id())
            .fixed_rect(rect)
            .title_bar(false)
            .show(ctx, |ui| {
                ui.set_min_width(width);

                ui.label(format!("FPS: {:.2}", self.fps));
                ui.label(format!("dt:  {:.2} ms", self.frame_time));
            })
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub path_count: usize,
    pub total_len: usize,
}

impl Widget for GraphStats {
    #[inline]
    fn id() -> &'static str {
        "graph_stats_box"
    }

    fn ui(
        &self,
        ctx: &egui::CtxRef,
        pos: Point,
        _size: Option<Point>,
    ) -> Option<egui::InnerResponse<Option<()>>> {
        egui::Window::new(Self::id())
            .title_bar(false)
            .collapsible(false)
            .auto_sized()
            .fixed_pos(pos)
            .show(ctx, |ui| {
                ui.label(format!("Nodes: {}", self.node_count));
                ui.label(format!("Edges: {}", self.edge_count));
                ui.label(format!("Paths: {}", self.path_count));
                ui.label(format!("Total length: {}", self.total_len));
            })
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GraphStatsMsg {
    pub node_count: Option<usize>,
    pub edge_count: Option<usize>,
    pub path_count: Option<usize>,
    pub total_len: Option<usize>,
}

impl GraphStats {
    pub fn apply_msg(&self, msg: GraphStatsMsg) -> Self {
        Self {
            node_count: msg.node_count.unwrap_or(self.node_count),
            edge_count: msg.edge_count.unwrap_or(self.edge_count),
            path_count: msg.path_count.unwrap_or(self.path_count),
            total_len: msg.total_len.unwrap_or(self.total_len),
        }
    }
}
