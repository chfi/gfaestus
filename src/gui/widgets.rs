#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use crate::geometry::*;
use crate::view::View;

pub trait Widget {
    fn id(&self) -> &str;

    fn ui(
        &self,
        ctx: &egui::CtxRef,
        pos: Point,
        size: Option<Point>,
    ) -> Option<egui::Response>;
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
    fn id(&self) -> &str {
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

        egui::Window::new(self.id())
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
pub struct ViewInfo {
    position: Point,
    view: View,
    mouse_screen: Point,
    mouse_world: Point,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub path_count: usize,
    pub total_len: usize,
}
