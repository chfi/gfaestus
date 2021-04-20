#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use rustc_hash::FxHashMap;

use crate::geometry::*;
use crate::view::View;

pub trait Widget {
    fn id() -> &'static str;

    fn ui(
        &self,
        ctx: &egui::CtxRef,
        pos: Point,
        size: Option<Point>,
    ) -> Option<egui::Response>;
}

pub struct MenuBar {}

impl MenuBar {
    pub const ID: &'static str = "app_menu_bar";

    pub fn ui<'a>(
        ctx: &egui::CtxRef,
        open_windows: &'a mut super::OpenWindows,
    ) {
        let settings = &mut open_windows.settings;

        let fps = &mut open_windows.fps;
        let graph_stats = &mut open_windows.graph_stats;

        let nodes = &mut open_windows.nodes;
        let paths = &mut open_windows.paths;

        let themes = &mut open_windows.themes;
        let overlays = &mut open_windows.overlays;

        egui::TopPanel::top(Self::ID).show(ctx, |ui| {
            if ui.selectable_label(*nodes, "Nodes").clicked() {
                *nodes = !*nodes;
            }

            if ui.selectable_label(*paths, "Paths").clicked() {
                *paths = !*paths;
            }

            if ui.selectable_label(*themes, "Themes").clicked() {
                *themes = !*themes;
            }

            if ui.selectable_label(*overlays, "Overlays").clicked() {
                *overlays = !*overlays;
            }
        });
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct NodeInfo {
    node_id: NodeId,
    len: usize,
    degree: (usize, usize),
    coverage: usize,
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

impl Widget for NodeSelection {
    #[inline]
    fn id() -> &'static str {
        "node_select_info"
    }

    fn ui(
        &self,
        ctx: &egui::CtxRef,
        pos: Point,
        size: Option<Point>,
    ) -> Option<egui::Response> {
        let scr = ctx.input().screen_rect();

        let size = size.unwrap_or(Point {
            x: pos.x + 200.0,
            y: pos.y + scr.max.y,
        });

        let rect = egui::Rect {
            min: pos.into(),
            max: size.into(),
        };

        egui::Window::new(Self::id())
            .fixed_rect(rect)
            .title_bar(false)
            .show(&ctx, |ui| {
                ui.expand_to_include_rect(rect);

                match &self {
                    NodeSelection::None => (),
                    NodeSelection::One { info } => {
                        let node_info = info;

                        let label =
                            format!("Selected node: {}", node_info.node_id.0);
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
            })
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FrameRate {
    fps: f32,
    frame_time: f32,
    frame: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct FrameRateMsg(FrameRate);

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
        size: Option<Point>,
    ) -> Option<egui::Response> {
        let scr = ctx.input().screen_rect();

        let size = size.unwrap_or(Point {
            x: pos.x + 200.0,
            y: pos.y + scr.max.y,
        });

        let rect = egui::Rect {
            min: pos.into(),
            max: size.into(),
        };

        egui::Window::new(Self::id())
            .fixed_rect(rect)
            .title_bar(false)
            .show(ctx, |ui| {
                ui.label(format!("FPS: {:.2}", self.fps));
                ui.label(format!("update time: {:.2} ms", self.frame_time));
            })
    }
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ViewInfo {
    position: Point,
    view: View,
    mouse_screen: Point,
    mouse_world: Point,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct ViewInfoMsg(ViewInfo);

#[derive(Debug, Default, Clone, Copy)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub path_count: usize,
    pub total_len: usize,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GraphStatsMsg {
    node_count: Option<usize>,
    edge_count: Option<usize>,
    path_count: Option<usize>,
    total_len: Option<usize>,
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
