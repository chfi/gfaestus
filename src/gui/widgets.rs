use crossbeam::{atomic::AtomicCell, channel::Sender};
#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use crate::{app::AppMsg, view::View};
use crate::{app::OverlayState, geometry::*};

pub trait Widget {
    fn id() -> &'static str;

    fn ui(
        &self,
        ctx: &egui::CtxRef,
        pos: Point,
        size: Option<Point>,
    ) -> Option<egui::Response>;
}

pub struct MenuBar {
    overlay_state: OverlayState,

    height: AtomicCell<f32>,
}

impl MenuBar {
    pub const ID: &'static str = "app_menu_bar";

    pub fn new(overlay_state: OverlayState) -> Self {
        Self {
            overlay_state,
            height: AtomicCell::new(0.0),
        }
    }

    pub fn height(&self) -> f32 {
        self.height.load()
    }

    pub fn ui<'a>(
        &self,
        ctx: &egui::CtxRef,
        open_windows: &'a mut super::OpenWindows,
        app_msg_tx: &Sender<AppMsg>,
    ) {
        let settings = &mut open_windows.settings;

        let fps = &mut open_windows.fps;
        let _graph_stats = &mut open_windows.graph_stats;

        let gff3 = &mut open_windows.gff3;
        let annotation_files = &mut open_windows.annotation_files;

        let nodes = &mut open_windows.nodes;
        let paths = &mut open_windows.paths;

        let _themes = &mut open_windows.themes;
        let overlays = &mut open_windows.overlays;

        let repl = &mut open_windows.repl_window;

        let resp = egui::TopBottomPanel::top(Self::ID).show(ctx, |ui| {
            // ui.horizontal(|ui| {

            use egui::menu;

            menu::bar(ui, |ui| {
                menu::menu(ui, "Graph", |ui| {
                    if ui.selectable_label(*nodes, "Nodes").clicked() {
                        *nodes = !*nodes;
                    }

                    if ui.selectable_label(*paths, "Paths").clicked() {
                        *paths = !*paths;
                    }
                });

                // if ui.selectable_label(*themes, "Themes").clicked() {
                //     *themes = !*themes;
                // }

                menu::menu(ui, "Annotations", |ui| {
                    if ui.selectable_label(*annotation_files, "Files").clicked()
                    {
                        *annotation_files = !*annotation_files;
                    }

                    if ui.selectable_label(*gff3, "GFF3").clicked() {
                        *gff3 = !*gff3;
                    }
                });

                menu::menu(ui, "Overlays", |ui| {
                    if ui.selectable_label(*overlays, "Overlay list").clicked()
                    {
                        *overlays = !*overlays;
                    }

                    if ui
                        .selectable_label(
                            self.overlay_state.use_overlay(),
                            "Show overlay",
                        )
                        .clicked()
                    {
                        self.overlay_state.toggle_overlay()
                    }
                });

                menu::menu(ui, "View", |ui| {
                    if ui.button("Goto selection").clicked() {
                        app_msg_tx.send(AppMsg::GotoSelection).unwrap();
                    }
                });

                menu::menu(ui, "Tools", |ui| {
                    if ui.button("REPL").clicked() {
                        *repl = !*repl;
                    }
                });

                menu::menu(ui, "Settings", |ui| {
                    if ui.selectable_label(*settings, "Settings").clicked() {
                        *settings = !*settings;
                    }

                    if ui.selectable_label(*fps, "Show FPS").clicked() {
                        *fps = !*fps;
                    }
                });
            });
        });

        let height = resp.response.rect.height();
        self.height.store(height);
    }

    pub fn ui_<'a>(
        &self,
        ctx: &egui::CtxRef,
        open_windows: &'a mut super::OpenWindows,
        app_msg_tx: &Sender<AppMsg>,
    ) {
        let settings = &mut open_windows.settings;

        let fps = &mut open_windows.fps;
        let _graph_stats = &mut open_windows.graph_stats;

        let gff3 = &mut open_windows.gff3;
        let annotation_files = &mut open_windows.annotation_files;

        let nodes = &mut open_windows.nodes;
        let paths = &mut open_windows.paths;

        let _themes = &mut open_windows.themes;
        let overlays = &mut open_windows.overlays;

        let repl = &mut open_windows.repl_window;

        let resp = egui::TopBottomPanel::top(Self::ID).show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.selectable_label(*nodes, "Nodes").clicked() {
                    *nodes = !*nodes;
                }

                if ui.selectable_label(*paths, "Paths").clicked() {
                    *paths = !*paths;
                }

                // if ui.selectable_label(*themes, "Themes").clicked() {
                //     *themes = !*themes;
                // }

                if ui.selectable_label(*overlays, "Overlays").clicked() {
                    *overlays = !*overlays;
                }

                if ui
                    .selectable_label(*annotation_files, "Annotations")
                    .clicked()
                {
                    *annotation_files = !*annotation_files;
                }

                // if ui.selectable_label(*gff3, "GFF3").clicked() {
                //     *gff3 = !*gff3;
                // }

                if ui.selectable_label(*fps, "FPS").clicked() {
                    *fps = !*fps;
                }

                if ui.selectable_label(*settings, "Settings").clicked() {
                    *settings = !*settings;
                }

                if ui
                    .selectable_label(
                        self.overlay_state.use_overlay(),
                        "Show overlay",
                    )
                    .clicked()
                {
                    self.overlay_state.toggle_overlay()
                }

                if ui.button("Goto selection").clicked() {
                    app_msg_tx.send(AppMsg::GotoSelection).unwrap();
                }

                ui.add_space(100.0);

                if ui.button("REPL").clicked() {
                    *repl = !*repl;
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
    ) -> Option<egui::Response> {
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
    ) -> Option<egui::Response> {
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
