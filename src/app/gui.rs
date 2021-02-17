#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use anyhow::{Context, Result};

use rgb::*;

use crate::geometry::*;
use crate::gfa::*;
use crate::input::*;
// use crate::layout::physics;
// use crate::layout::*;
use crate::render::*;
use crate::ui::{UICmd, UIState, UIThread};
use crate::view;
use crate::view::View;

// struct GraphViewActiveSet {
//     node_tooltip: bool,
//     node_info: bool,
//     graph_summary_stats: bool,
//     view_info: bool,
// }

/*
#[derive(Debug, Default, Clone, PartialEq)]
struct NodeInfo {
    // node_id: Option<NodeId>,
    sequence: Vec<u8>,
    paths: Vec<PathId>,
    neighbors: Vec<NodeId>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct GraphSummaryStats {
    node_count: usize,
    edge_count: usize,
    path_count: usize,
    total_len: usize,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct GraphView {
    node_hover: Option<NodeId>,
    node_selected: Option<NodeId>,
    node_selected_info: NodeInfo,
    graph_summary_stats: GraphSummaryStats,
    frame: usize,
    mouse_pos: Point,
}
*/

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct NodeHoverTooltip {
    node_id: Option<NodeId>,
    offset: Point,
}

#[derive(Debug, Default, Clone, PartialEq)]
struct NodeInfoBox {
    node_id: Option<NodeId>,
    sequence: Vec<u8>,
    paths: Vec<Vec<u8>>,
    neighbors: Vec<NodeId>,
}

#[derive(Default, Clone, PartialEq)]
pub struct GfaestusGui {
    ctx: egui::CtxRef,
    events: Vec<egui::Event>,
    // raw_input
    // ref to PackedGraph?
    node_hover: NodeHoverTooltip,
    node_info_box: NodeInfoBox,

    mouse_pos: Point,
    window_dims: Point,
}

/*

impl GfaestusGui {
    // pub fn set_screen_rect(&mut self, width: f32, height: f32) {
    // }
    pub fn add_event(&mut self, event: egui::Event) {
        self.events.push(event);
    }

    pub fn begin_frame(&mut self, screen_dims: Option<Point>) {
        let mut raw_input = egui::RawInput::default();

        let screen_rect = screen_dims.map(|p| egui::Rect {
            min: egui::Pos2 { x: 0.0, y: 0.0 },
            max: egui::Pos2 { x: p.x, y: p.y },
        });

        raw_input.screen_rect = screen_rect;
        raw_input.events = std::mem::take(&mut self.events);

        self.ctx.begin_frame(raw_input);

        if let Some(node_id) = self.node_hover.node_id.as_ref() {
            let pos = egui::pos2(
                (self.mouse_pos.x - 32.0).max(0.0).min(width),
                (self.mouse_pos.y - 24.0).max(0.0).min(height),
            );

            egui::Area::new("node_hover_tooltip")
                .fixed_pos(pos)
                .show(&egui_ctx, |ui| {
                    ui.label(node_id.0.to_string());
                });
        }
    }
}
*/
