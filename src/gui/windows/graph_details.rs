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

use crossbeam::{atomic::AtomicCell, channel::Sender};
use std::sync::Arc;

use bstr::ByteSlice;

use crate::graph_query::GraphQuery;
use crate::{app::AppMsg, geometry::*};

#[derive(Debug, Clone)]
pub struct NodeDetails {
    node_id: Arc<AtomicCell<Option<NodeId>>>,
    fetched_node: Option<NodeId>,

    sequence: Vec<u8>,
    degree: (usize, usize),
    paths: Vec<(PathId, StepPtr, usize)>,

    unique_paths: Vec<PathId>,
}

impl std::default::Default for NodeDetails {
    fn default() -> Self {
        Self {
            node_id: Arc::new(None.into()),
            fetched_node: None,
            sequence: Vec::new(),
            degree: (0, 0),
            paths: Vec::new(),
            unique_paths: Vec::new(),
        }
    }
}

pub enum NodeDetailsMsg {
    SetNode(NodeId),
    NoNode,
}

impl NodeDetails {
    const ID: &'static str = "node_details_window";

    pub fn node_id_cell(&self) -> &Arc<AtomicCell<Option<NodeId>>> {
        &self.node_id
    }

    pub fn apply_msg(&mut self, msg: NodeDetailsMsg) {
        match msg {
            NodeDetailsMsg::SetNode(node_id) => {
                self.node_id.store(Some(node_id));
            }
            NodeDetailsMsg::NoNode => {
                self.node_id.store(None);
                self.sequence.clear();
                self.degree = (0, 0);
                self.paths.clear();
            }
        }
    }

    pub fn need_fetch(&self) -> bool {
        let to_show = self.node_id.load();
        to_show != self.fetched_node
    }

    pub fn fetch(&mut self, graph_query: &GraphQuery) -> Option<()> {
        if !self.need_fetch() {
            return None;
        }

        let node_id = self.node_id.load()?;

        self.sequence.clear();
        self.degree = (0, 0);
        self.paths.clear();
        self.unique_paths.clear();

        let graph = graph_query.graph();

        let handle = Handle::pack(node_id, false);

        self.sequence.extend(graph.sequence(handle));

        let degree_l = graph.neighbors(handle, Direction::Left).count();
        let degree_r = graph.neighbors(handle, Direction::Right).count();

        self.degree = (degree_l, degree_r);

        let paths_fwd =
            graph_query.handle_positions(Handle::pack(node_id, false));
        // let paths_rev = graph_query.handle_positions(Handle::pack(node_id, true));

        if let Some(p) = paths_fwd {
            self.paths.extend_from_slice(&p);

            self.unique_paths
                .extend(self.paths.iter().map(|(path, _, _)| path));
            self.unique_paths.sort();
            self.unique_paths.dedup();
        }
        // if let Some(p) = paths_rev {
        //     self.paths.extend_from_slice(&p);
        // }

        self.fetched_node = Some(node_id);

        Some(())
    }

    pub fn ui(
        &mut self,
        open_node_details: &mut bool,
        graph_query: &GraphQuery,
        ctx: &egui::CtxRef,
        path_details_id_cell: &AtomicCell<Option<PathId>>,
        open_path_details: &mut bool,
    ) -> Option<egui::Response> {
        if self.need_fetch() {
            self.fetch(graph_query);
        }

        egui::Window::new("Node details")
            .id(egui::Id::new(Self::ID))
            .default_pos(egui::Pos2::new(450.0, 200.0))
            .open(open_node_details)
            .show(ctx, |mut ui| {
                if let Some(node_id) = self.node_id.load() {
                    ui.set_min_height(200.0);
                    ui.set_max_width(200.0);

                    ui.label(format!("Node {}", node_id));

                    ui.separator();

                    if self.sequence.len() < 50 {
                        ui.label(format!("Seq: {}", self.sequence.as_bstr()));
                    } else {
                        ui.label(format!("Seq len: {}", self.sequence.len()));
                    }

                    ui.label(format!(
                        "Degree ({}, {})",
                        self.degree.0, self.degree.1
                    ));

                    ui.separator();

                    let separator = || egui::Separator::default().spacing(1.0);

                    egui::ScrollArea::auto_sized().show(&mut ui, |mut ui| {
                        egui::Grid::new("node_details_path_list")
                            .spacing(Point { x: 10.0, y: 5.0 })
                            .striped(true)
                            .show(&mut ui, |ui| {
                                ui.label("Path");
                                ui.add(separator());

                                ui.label("Step");
                                ui.add(separator());

                                ui.label("Base pos");
                                ui.end_row();

                                for (path_id, step_ptr, pos) in
                                    self.paths.iter()
                                {
                                    let path_name = graph_query
                                        .graph()
                                        .get_path_name_vec(*path_id);

                                    let mut row = if let Some(name) = path_name
                                    {
                                        ui.label(format!("{}", name.as_bstr()))
                                    } else {
                                        ui.label(format!(
                                            "Path ID {}",
                                            path_id.0
                                        ))
                                    };

                                    row = row.union(ui.add(separator()));

                                    row = row.union(ui.label(format!(
                                        "{}",
                                        step_ptr.to_vector_value()
                                    )));
                                    row = row.union(ui.add(separator()));

                                    row =
                                        row.union(ui.label(format!("{}", pos)));

                                    let row_interact = ui.interact(
                                        row.rect,
                                        egui::Id::new(ui.id().with(format!(
                                            "path_{}_{}",
                                            path_id.0,
                                            step_ptr.to_vector_value()
                                        ))),
                                        egui::Sense::click(),
                                    );

                                    if row_interact.clicked() {
                                        path_details_id_cell
                                            .store(Some(*path_id));
                                        *open_path_details = true;
                                    }

                                    ui.end_row();
                                }
                            });
                    });
                    ui.shrink_width_to_current();
                } else {
                    ui.label("Examine a node by picking it from the node list");
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
    unique_paths: Vec<PathId>,
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

        let paths_fwd =
            graph_query.handle_positions(Handle::pack(node_id, false));
        let paths_rev =
            graph_query.handle_positions(Handle::pack(node_id, true));

        let paths_len = paths_fwd.as_ref().map(|v| v.len()).unwrap_or_default()
            + paths_rev.as_ref().map(|v| v.len()).unwrap_or_default();

        let mut paths = Vec::with_capacity(paths_len);
        let mut unique_paths = Vec::with_capacity(paths_len);
        if let Some(p) = paths_fwd {
            paths.extend_from_slice(&p);
            unique_paths.extend(p.iter().map(|(path, _, _)| path));
        }
        if let Some(p) = paths_rev {
            paths.extend_from_slice(&p);
            unique_paths.extend(p.iter().map(|(path, _, _)| path));
        }

        unique_paths.sort();
        unique_paths.dedup();

        Self {
            node_id,
            sequence,
            degree,
            paths,
            unique_paths,

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
    page_size: usize,
    page_count: usize,

    slots: Vec<NodeListSlot>,

    update_slots: bool,

    apply_filter: AtomicCell<bool>,

    node_details_id: Arc<AtomicCell<Option<NodeId>>>,
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
        node_details_id: Arc<AtomicCell<Option<NodeId>>>,
    ) -> Self {
        let graph = graph_query.graph();
        let node_count = graph.node_count();

        let page_size = page_size.min(node_count);

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

            apply_filter: true.into(),

            node_details_id,
        }
    }

    pub fn set_filtered(&mut self, nodes: &[NodeId]) {
        self.filtered_nodes.clear();
        self.filtered_nodes.extend(nodes.iter().copied());

        if self.apply_filter.load() {
            self.update_slots = true;
        }
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        app_msg_tx: &Sender<AppMsg>,
        open_node_details: &mut bool,
        graph_query: &GraphQuery,
    ) -> Option<egui::Response> {
        let filter = self.apply_filter.load();

        let nodes = if !filter || self.filtered_nodes.is_empty() {
            &self.all_nodes
        } else {
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
                slot.unique_paths.clear();

                let paths_fwd = graph_query.handle_positions(handle);
                let paths_rev = graph_query.handle_positions(handle.flip());

                if let Some(p) = paths_fwd {
                    slot.paths.extend_from_slice(&p);
                    slot.unique_paths.extend(p.iter().map(|(path, _, _)| path));
                }
                if let Some(p) = paths_rev {
                    slot.paths.extend_from_slice(&p);
                    slot.unique_paths.extend(p.iter().map(|(path, _, _)| path));
                }

                slot.unique_paths.sort();
                slot.unique_paths.dedup();
            }

            self.update_slots = false;
        }

        egui::Window::new("Nodes")
            // .enabled(*show)
            .id(egui::Id::new(Self::ID))
            .default_pos(egui::Pos2::new(200.0, 200.0))
            .show(ctx, |mut ui| {
                ui.set_min_height(300.0);
                ui.set_max_width(200.0);

                ui.horizontal(|ui| {
                    let clear_selection_btn = ui
                        .button("Clear selection")
                        .on_hover_text("Hotkey: <Escape>");

                    if clear_selection_btn.clicked() {
                        use crate::app::Select;
                        app_msg_tx
                            .send(AppMsg::Selection(Select::Clear))
                            .unwrap();
                    }

                    if ui
                        .selectable_label(*open_node_details, "Node Details")
                        .clicked()
                    {
                        *open_node_details = !*open_node_details;
                    }
                });

                let page = &mut self.page;
                let page_count = self.page_count;
                let update_slots = &mut self.update_slots;

                let apply_filter = &self.apply_filter;

                if ui.selectable_label(filter, "Show only selected").clicked() {
                    apply_filter.store(!filter);
                    *update_slots = true;
                }

                ui.label(format!("Page {}/{}", *page + 1, page_count + 1));

                ui.horizontal(|ui| {
                    if ui.button("First").clicked() {
                        if *page != 0 {
                            *page = 0;
                            *update_slots = true;
                        }
                    }

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

                    if ui.button("Last").clicked() {
                        if *page != page_count {
                            *page = page_count;
                            *update_slots = true;
                        }
                    }
                });

                let node_id_cell = &self.node_details_id;

                egui::ScrollArea::auto_sized().show(&mut ui, |mut ui| {
                    egui::Grid::new("node_list_grid").striped(true).show(
                        &mut ui,
                        |ui| {
                            ui.label("Node");
                            ui.label("Degree");
                            ui.label("Seq. len");
                            ui.label("Path count");
                            ui.end_row();

                            for (ix, slot) in self.slots.iter().enumerate() {
                                if slot.visible {
                                    let mut row =
                                        ui.label(format!("{}", slot.node_id));

                                    row = row.union(ui.label(format!(
                                        "({}, {})",
                                        slot.degree.0, slot.degree.1
                                    )));

                                    row = row.union(ui.label(format!(
                                        "{}",
                                        slot.sequence.len()
                                    )));

                                    row = row.union(ui.label(format!(
                                        "{}",
                                        slot.unique_paths.len() // slot.paths.len()
                                    )));

                                    let row_interact = ui.interact(
                                        row.rect,
                                        egui::Id::new(ui.id().with(ix)),
                                        egui::Sense::click(),
                                    );

                                    if row_interact.clicked() {
                                        node_id_cell.store(Some(slot.node_id));

                                        *open_node_details = true;
                                    }

                                    ui.end_row();
                                }
                            }
                        },
                    );
                });

                ui.shrink_width_to_current();
            })
    }
}
