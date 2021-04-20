#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    packedgraph::*,
    pathhandlegraph::*,
};

use crossbeam::atomic::AtomicCell;

use crate::geometry::*;
use crate::graph_query::{GraphQuery, GraphQueryRequest, GraphQueryResp};
use crate::view::View;

#[derive(Debug, Clone)]
pub struct NodeDetails {
    node_id: NodeId,
    sequence: Vec<u8>,
    degree: (usize, usize),
    paths: Vec<PathId>,

    visible: bool,
}

impl NodeDetails {
    pub fn from_id(graph: &PackedGraph, node_id: NodeId) -> Self {
        let visible = true;

        let handle = Handle::pack(node_id, false);

        let sequence = graph.sequence_vec(handle);

        let degree_l = graph.neighbors(handle, Direction::Left).count();
        let degree_r = graph.neighbors(handle, Direction::Right).count();

        let degree = (degree_l, degree_r);

        let paths = graph
            .steps_on_handle(handle)
            .into_iter()
            .flatten()
            .map(|(path, _)| path)
            .collect();

        Self {
            node_id,
            sequence,
            degree,
            paths,

            visible,
        }
    }
}

// pub struct

#[derive(Debug)]
pub struct NodeList {
    // probably not needed as I can assume compact node IDs
    all_nodes: Vec<NodeId>,

    // filtered_nodes: Option<Vec<NodeId>>,
    filtered_nodes: Vec<NodeId>,

    page: usize,
    page_count: usize,
    page_size: usize,

    // top: usize,
    // bottom: usize,
    slots: Vec<NodeDetails>,

    update_slots: bool,

    apply_filter: AtomicCell<bool>,
}

pub enum NodeListMsg {
    ApplyFilter(Option<bool>),
    NextPage,
    PrevPage,
    SetPage(usize),
    SetFiltered(Vec<NodeId>),
}

impl NodeList {
    const ID: &'static str = "node_list_window";

    pub fn apply_msg(&mut self, msg: NodeListMsg) {
        match msg {
            NodeListMsg::ApplyFilter(apply) => {
                if let Some(apply) = apply {
                    self.apply_filter.store(apply);
                    // TODO only update when necessary
                    self.update_slots = true;
                } else {
                    self.apply_filter.fetch_xor(true);
                    self.update_slots = true;
                }
            }
            NodeListMsg::NextPage => {
                if self.page < self.page_count {
                    self.page += 1;
                    self.update_slots = true;
                }
            }
            NodeListMsg::PrevPage => {
                if self.page > 0 {
                    self.page -= 1;
                    self.update_slots = true;
                }
            }
            NodeListMsg::SetPage(page) => {
                let page = page.clamp(0, self.page_count);
                if self.page != page {
                    self.page = page;
                    self.update_slots = true;
                }
            }
            NodeListMsg::SetFiltered(mut nodes) => {
                std::mem::swap(&mut nodes, &mut self.filtered_nodes);
                self.update_slots = true;
            }
        }
    }

    pub fn new(graph_query: &GraphQuery, page_size: usize) -> Self {
        let graph = graph_query.graph();
        let node_count = graph.node_count();

        let mut all_nodes = graph.handles().map(|h| h.id()).collect::<Vec<_>>();
        all_nodes.sort();

        let page_count = node_count / page_size;

        let filtered_nodes: Vec<NodeId> = Vec::new();

        let mut slots: Vec<NodeDetails> = Vec::with_capacity(page_size);

        for &node in all_nodes[0..page_size].iter() {
            let slot = NodeDetails::from_id(graph, node);

            slots.push(slot);
        }

        Self {
            all_nodes,
            filtered_nodes,

            page: 0,
            page_count,
            page_size,

            slots,

            update_slots: false,

            apply_filter: AtomicCell::new(false),
        }
    }

    pub fn set_filtered(&mut self, nodes: &[NodeId]) {
        self.filtered_nodes.clear();
        self.filtered_nodes.extend(nodes.iter().copied());

        if self.filtered_nodes.is_empty() {
            self.apply_filter.store(false);
        }

        if self.apply_filter.load() {
            self.update_slots = true;
        }
    }

    pub fn ui(
        &mut self,
        graph_query: &GraphQuery,
        ctx: &egui::CtxRef,
        show: &mut bool,
    ) -> Option<egui::Response> {
        let mut filter = self.apply_filter.load();

        let nodes = if !filter || self.filtered_nodes.is_empty() {
            filter = false;
            &self.all_nodes
        } else {
            filter = true;
            &self.filtered_nodes
        };

        self.page_count = nodes.len() / self.page_size;

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

        egui::Window::new("Nodes")
            .id(egui::Id::new(Self::ID))
            .show(ctx, |ui| {
                if ui.selectable_label(filter, "Filter").clicked() {
                    self.apply_filter.store(!filter);
                    self.update_slots = true;
                }

                let label = format!("Node - Degree - Seq. len - Paths");
                ui.label(label);

                for slot in self.slots.iter() {
                    if slot.visible {
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
