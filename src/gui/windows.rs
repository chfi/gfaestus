#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use crate::geometry::*;
use crate::graph_query::{GraphQuery, GraphQueryRequest, GraphQueryResp};
use crate::view::View;

pub struct NodeDetails {
    node_id: NodeId,
    sequence: Vec<u8>,
    degree: (usize, usize),
    paths: Vec<PathId>,

    visible: bool,
}

pub struct NodeList {
    // probably not needed as I can assume compact node IDs
    all_nodes: Vec<NodeId>,

    filtered_nodes: Option<Vec<NodeId>>,

    page: usize,
    page_count: usize,
    page_size: usize,

    // top: usize,
    // bottom: usize,
    slots: Vec<NodeDetails>,

    update_slots: bool,
}

impl NodeList {
    const ID: &'static str = "node_list_window";

    pub fn ui(
        &mut self,
        graph_query: &GraphQuery,
        ctx: &egui::CtxRef,
        show: &mut bool,
    ) -> Option<egui::Response> {
        let nodes = self.filtered_nodes.as_ref().unwrap_or(&self.all_nodes);

        // this'll need fixing
        // let start =
        //     (self.page * self.page_size).min(nodes.len() - self.page_size);
        // let end = start + self.page_size;

        if self.update_slots {
            let page_start = (self.page * self.page_size)
                .min(nodes.len() - (nodes.len() % self.page_size));
            let page_end = (page_start + self.page_size).min(nodes.len());

            for slot in self.slots.iter_mut() {
                slot.visible = false;
            }

            for (slot, node) in
                self.slots.iter_mut().zip(&nodes[page_start..page_end])
            {
                slot.visible = true;

                slot.node_id = *node;

                let handle = Handle::pack(*node, false);

                slot.sequence.clear();
                slot.sequence.extend(graph_query.graph().sequence(handle));

                slot.paths.clear();
                slot.paths.extend(
                    graph_query
                        .graph()
                        .steps_on_handle(handle)
                        .into_iter()
                        .flatten()
                        .map(|(path, _)| path),
                );
            }

            self.update_slots = false;
        }

        egui::Window::new(Self::ID).show(ctx, |ui| {
            for slot in self.slots.iter() {
                let label = format!(
                    "{} - ({}, {}) - {} - {}",
                    slot.node_id,
                    slot.degree.0,
                    slot.degree.1,
                    slot.sequence.len(),
                    slot.paths.len()
                );

                ui.label(label);
            }

            if ui.button("Prev").clicked() {
                if self.page > 0 {
                    self.page -= 1;
                    self.update_slots = true;
                }
            }

            if ui.button("Next").clicked() {
                if self.page < self.page_count {
                    self.page += 1;
                    self.update_slots = true;
                }
            }
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ActiveDetails {
    Overview,
    NodeList,
    // PathList,
    // NodeDetails,
    // PathDetails,
}

pub struct GraphDetails {
    active: ActiveDetails,

    filtered_nodes: Option<Vec<NodeId>>,
    // filtered_paths: Option<Vec<PathId>>,

    // node_details: Option<NodeId>,
    // path_details: Option<PathId>,
}

impl GraphDetails {
    const ID: &'static str = "graph_details_window";

    pub fn ui(
        &self,
        ctx: &egui::CtxRef,
        show: &mut bool,
    ) -> Option<egui::Response> {
        unimplemented!();
    }
}
