use handlegraph::packedgraph::paths::StepPtr;
#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    packedgraph::index::OneBasedIndex,
    packedgraph::*,
    path_position::*,
    pathhandlegraph::*,
};

use crossbeam::atomic::AtomicCell;

use bstr::{BStr, ByteSlice};

use crate::geometry::*;
use crate::graph_query::{GraphQuery, GraphQueryRequest, GraphQueryResp};
use crate::view::View;

#[derive(Debug, Clone)]
pub struct NodeDetails {
    node_id: Option<NodeId>,
    sequence: Vec<u8>,
    degree: (usize, usize),
    paths: Vec<(PathId, StepPtr, usize)>,

    need_fetch: bool,
}

impl std::default::Default for NodeDetails {
    fn default() -> Self {
        Self {
            node_id: None,
            sequence: Vec::new(),
            degree: (0, 0),
            paths: Vec::new(),
            need_fetch: false,
        }
    }
}

pub enum NodeDetailsMsg {
    SetNode(NodeId),
    NoNode,
}

impl NodeDetails {
    const ID: &'static str = "node_details_window";

    pub fn apply_msg(&mut self, msg: NodeDetailsMsg) {
        match msg {
            NodeDetailsMsg::SetNode(node_id) => {
                self.node_id = Some(node_id);
                self.need_fetch = true;
            }
            NodeDetailsMsg::NoNode => {
                self.node_id = None;
                self.sequence.clear();
                self.degree = (0, 0);
                self.paths.clear();
                self.need_fetch = false;
            }
        }
    }

    pub fn fetch(&mut self, graph_query: &GraphQuery) -> Option<()> {
        if !self.need_fetch {
            return None;
        }

        let node_id = self.node_id?;

        self.sequence.clear();
        self.degree = (0, 0);
        self.paths.clear();
        self.need_fetch = false;

        let graph = graph_query.graph();

        let handle = Handle::pack(node_id, false);

        self.sequence.extend(graph.sequence(handle));

        let degree_l = graph.neighbors(handle, Direction::Left).count();
        let degree_r = graph.neighbors(handle, Direction::Right).count();

        self.degree = (degree_l, degree_r);

        let paths_fwd = graph_query.handle_positions(Handle::pack(node_id, false));
        let paths_rev = graph_query.handle_positions(Handle::pack(node_id, true));

        if let Some(p) = paths_fwd {
            self.paths.extend_from_slice(&p);
        }
        if let Some(p) = paths_rev {
            self.paths.extend_from_slice(&p);
        }

        Some(())
    }

    pub fn ui(
        &mut self,
        graph_query: &GraphQuery,
        ctx: &egui::CtxRef,
        // show: &mut bool
    ) -> Option<egui::Response> {
        if self.need_fetch {
            self.fetch(graph_query);
        }

        egui::Window::new("Node details")
            .id(egui::Id::new(Self::ID))
            .show(ctx, |mut ui| {
                if let Some(node_id) = self.node_id {
                    ui.set_min_height(400.0);

                    ui.label(format!("Node {}", node_id));

                    ui.separator();

                    if self.sequence.len() < 50 {
                        ui.label(format!("Seq: {}", self.sequence.as_bstr()));
                    } else {
                        ui.label(format!("Seq len: {}", self.sequence.len()));
                    }

                    ui.label(format!("Degree ({}, {})", self.degree.0, self.degree.1));

                    ui.separator();

                    egui::ScrollArea::auto_sized().show(&mut ui, |mut ui| {
                        egui::Grid::new("node_details_path_list")
                            .striped(true)
                            .show(&mut ui, |ui| {
                                ui.label("Path");
                                ui.separator();
                                ui.label("Step");
                                ui.separator();
                                ui.label("Base pos");
                                ui.end_row();

                                for (path_id, step_ptr, pos) in self.paths.iter() {
                                    let path_name = graph_query.graph().get_path_name_vec(*path_id);

                                    if let Some(name) = path_name {
                                        ui.label(format!("{}", name.as_bstr()));
                                    } else {
                                        ui.label(format!("Path ID {}", path_id.0));
                                    }

                                    ui.separator();

                                    ui.label(format!("{}", step_ptr.to_vector_value()));
                                    ui.separator();
                                    ui.label(format!("{}", pos));

                                    ui.end_row();
                                }
                            });
                    });
                } else {
                    ui.label("No node");
                }
            })
    }
}

#[derive(Debug, Clone)]
pub struct NodeListSlot {
    node_id: NodeId,
    sequence: Vec<u8>,
    degree: (usize, usize),

    paths: Vec<(PathId, StepPtr, usize)>,
    visible: bool,
}

impl NodeListSlot {
    pub fn from_id(graph_query: &GraphQuery, node_id: NodeId) -> Self {
        let visible = true;

        let graph = graph_query.graph();

        let handle = Handle::pack(node_id, false);

        let sequence = graph.sequence_vec(handle);

        let degree_l = graph.neighbors(handle, Direction::Left).count();
        let degree_r = graph.neighbors(handle, Direction::Right).count();

        let degree = (degree_l, degree_r);

        let paths_fwd = graph_query.handle_positions(Handle::pack(node_id, false));
        let paths_rev = graph_query.handle_positions(Handle::pack(node_id, true));

        let paths_len = paths_fwd.as_ref().map(|v| v.len()).unwrap_or_default()
            + paths_rev.as_ref().map(|v| v.len()).unwrap_or_default();

        let mut paths = Vec::with_capacity(paths_len);
        if let Some(p) = paths_fwd {
            paths.extend_from_slice(&p);
        }
        if let Some(p) = paths_rev {
            paths.extend_from_slice(&p);
        }

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
    slots: Vec<NodeListSlot>,

    update_slots: bool,

    apply_filter: AtomicCell<bool>,

    node_details_tx: crossbeam::channel::Sender<NodeDetailsMsg>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
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
            NodeListMsg::SetFiltered(nodes) => {
                self.set_filtered(&nodes);
                // std::mem::swap(&mut nodes, &mut self.filtered_nodes);
                self.update_slots = true;
            }
        }
    }

    pub fn new(
        graph_query: &GraphQuery,
        page_size: usize,
        node_details_tx: crossbeam::channel::Sender<NodeDetailsMsg>,
    ) -> Self {
        let graph = graph_query.graph();
        let node_count = graph.node_count();

        let mut all_nodes = graph.handles().map(|h| h.id()).collect::<Vec<_>>();
        all_nodes.sort();

        let page_count = node_count / page_size;

        let filtered_nodes: Vec<NodeId> = Vec::new();

        let mut slots: Vec<NodeListSlot> = Vec::with_capacity(page_size);

        for &node in all_nodes[0..page_size].iter() {
            let slot = NodeListSlot::from_id(graph_query, node);

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

            node_details_tx,
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
            let page_start =
                (self.page * self.page_size).min(nodes.len() - (nodes.len() % self.page_size));
            let page_end = (page_start + self.page_size).min(nodes.len());

            for slot in self.slots.iter_mut() {
                slot.visible = false;
            }

            for (slot, node) in self.slots.iter_mut().zip(&nodes[page_start..page_end]) {
                slot.visible = true;

                slot.node_id = *node;

                let handle = Handle::pack(*node, false);

                slot.sequence.clear();
                slot.sequence.extend(graph_query.graph().sequence(handle));

                slot.paths.clear();

                let paths_fwd = graph_query.handle_positions(handle);
                let paths_rev = graph_query.handle_positions(handle.flip());

                if let Some(p) = paths_fwd {
                    slot.paths.extend_from_slice(&p);
                }
                if let Some(p) = paths_rev {
                    slot.paths.extend_from_slice(&p);
                }
            }

            self.update_slots = false;
        }

        egui::Window::new("Nodes")
            // .enabled(*show)
            .id(egui::Id::new(Self::ID))
            .show(ctx, |mut ui| {
                ui.set_min_height(400.0);

                if ui.selectable_label(filter, "Filter").clicked() {
                    self.apply_filter.store(!filter);
                    self.update_slots = true;
                }

                let tx_chn = &self.node_details_tx;

                let page = &mut self.page;
                let page_count = self.page_count;
                let update_slots = &mut self.update_slots;

                ui.horizontal(|ui| {
                    if ui.button("Prev").clicked() {
                        if *page > 0 {
                            *page -= 1;
                            *update_slots = true;
                        }
                    }

                    if ui.button("Next").clicked() {
                        if *page < page_count {
                            *page += 1;
                            *update_slots = true;
                        }
                    }
                });

                egui::ScrollArea::auto_sized().show(&mut ui, |mut ui| {
                    egui::Grid::new("node_list_grid")
                        .striped(true)
                        .show(&mut ui, |ui| {
                            ui.label("Node");
                            ui.separator();
                            ui.label("Degree");
                            ui.separator();
                            ui.label("Seq. len");
                            ui.separator();
                            ui.label("Path count");
                            ui.end_row();

                            for slot in self.slots.iter() {
                                if slot.visible {
                                    let mut row = ui.label(format!("{}", slot.node_id));

                                    row = row.union(ui.separator());
                                    row = row.union(
                                        ui.label(format!("({}, {})", slot.degree.0, slot.degree.1)),
                                    );

                                    row = row.union(ui.separator());
                                    row = row.union(ui.label(format!("{}", slot.sequence.len())));

                                    row = row.union(ui.separator());
                                    row = row.union(ui.label(format!("{}", slot.paths.len())));
                                    if row.clicked() {
                                        tx_chn.send(NodeDetailsMsg::SetNode(slot.node_id)).unwrap();
                                    }

                                    ui.end_row();
                                }
                            }
                        });
                });
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

    pub fn ui(&self, ctx: &egui::CtxRef, show: &mut bool) -> Option<egui::Response> {
        unimplemented!();
    }
}
