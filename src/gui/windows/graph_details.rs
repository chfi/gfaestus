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
use rustc_hash::FxHashSet;
use std::sync::Arc;

use bstr::ByteSlice;

use crate::{
    app::AppMsg, context::ContextMgr, geometry::*, gui::util::ColumnWidths,
};

use crate::gui::util as gui_util;

use crate::{graph_query::GraphQuery, gui::util::grid_row_label};

pub struct NodeDetails {
    node_id: Arc<AtomicCell<Option<NodeId>>>,
    fetched_node: Option<NodeId>,

    sequence: Vec<u8>,
    degree: (usize, usize),
    paths: Vec<(PathId, StepPtr, usize)>,

    unique_paths: Vec<PathId>,

    col_widths: ColumnWidths<3>,
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

            col_widths: Default::default(),
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

        if let Some(p) = paths_fwd {
            self.paths.extend_from_slice(&p);

            self.unique_paths
                .extend(self.paths.iter().map(|(path, _, _)| path));
            self.unique_paths.sort();
            self.unique_paths.dedup();
        }

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
        ctx_mgr: &ContextMgr,
    ) -> Option<egui::InnerResponse<Option<()>>> {
        if self.need_fetch() {
            self.fetch(graph_query);
        }

        egui::Window::new("Node details")
            .id(egui::Id::new(Self::ID))
            .default_pos(egui::Pos2::new(450.0, 200.0))
            .open(open_node_details)
            .show(ctx, |ui| {
                if let Some(node_id) = self.node_id.load() {
                    ui.set_min_height(200.0);
                    ui.set_max_width(200.0);

                    let node_label = ui.add(
                        egui::Label::new(format!("Node {}", node_id))
                            .sense(egui::Sense::click()),
                    );

                    if node_label.hovered() {
                        ctx_mgr.produce_context(|| node_id);
                    }

                    // if node_label.clicked_by(egui::PointerButton::Secondary) {
                    //     ctx_tx.send(ContextEntry::Node(node_id)).unwrap();
                    // }

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

                    let scroll_align = gui_util::add_scroll_buttons(ui);

                    let num_rows = self.paths.len();
                    let text_style = egui::TextStyle::Body;
                    let row_height = ui.fonts()[text_style].row_height();

                    let [w0, w1, w2] = self.col_widths.get();

                    let header = egui::Grid::new(
                        "node_details_path_list_header",
                    )
                    .show(ui, |ui| {
                        let inner = grid_row_label(
                            ui,
                            egui::Id::new("node_details_path_list_header__"),
                            &["Path", "Step", "Base pos"],
                            false,
                            Some(&[w0, w1, w2]),
                        );
                        self.col_widths.set_hdr(&inner.inner);
                    });

                    gui_util::scrolled_area(ui, num_rows, scroll_align)
                        .show_rows(ui, row_height, num_rows, |ui, range| {
                            ui.set_min_width(header.response.rect.width());

                            egui::Grid::new("node_details_path_list")
                                .spacing(Point { x: 10.0, y: 5.0 })
                                .striped(true)
                                .show(ui, |ui| {
                                    let take_n = range.start.max(range.end)
                                        - range.start;

                                    for (path_id, step_ptr, pos) in self
                                        .paths
                                        .iter()
                                        .skip(range.start)
                                        .take(take_n)
                                    {
                                        let path_name = graph_query
                                            .graph()
                                            .get_path_name_vec(*path_id);

                                        let name = if let Some(name) = path_name
                                        {
                                            format!("{}", name.as_bstr())
                                        } else {
                                            format!("Path ID {}", path_id.0)
                                        };

                                        let step_str = format!(
                                            "{}",
                                            step_ptr.to_vector_value()
                                        );

                                        let pos_str = format!("{}", pos);

                                        let fields: [&str; 3] =
                                            [&name, &step_str, &pos_str];

                                        let inner = grid_row_label(
                                            ui,
                                            egui::Id::new(ui.id().with(
                                                format!(
                                                    "path_{}_{}",
                                                    path_id.0,
                                                    step_ptr.to_vector_value()
                                                ),
                                            )),
                                            &fields,
                                            false,
                                            Some(&[w0, w1, w2]),
                                        );

                                        self.col_widths.set(&inner.inner);

                                        let row = inner.response;

                                        if row.clicked() {
                                            path_details_id_cell
                                                .store(Some(*path_id));
                                            *open_path_details = true;
                                        }

                                        if row.hovered() {
                                            ctx_mgr
                                                .produce_context(|| *path_id);
                                        }

                                        /*
                                        if row.clicked_by(
                                            egui::PointerButton::Secondary,
                                        ) {
                                            ctx_tx
                                                .send(ContextEntry::Path(
                                                    *path_id,
                                                ))
                                                .unwrap();
                                        }
                                        */
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

pub struct NodeList {
    // probably not needed as I can assume compact node IDs
    all_nodes: Vec<NodeId>,

    filtered_nodes: Vec<NodeId>,

    apply_filter: AtomicCell<bool>,

    node_details_id: Arc<AtomicCell<Option<NodeId>>>,

    range: AtomicCell<(usize, usize)>,

    col_widths: ColumnWidths<5>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NodeListMsg {
    ApplyFilter(Option<bool>),
    SetFiltered(Vec<NodeId>),
}

impl NodeList {
    const ID: &'static str = "node_list_window";

    pub fn apply_msg(&mut self, msg: NodeListMsg) {
        match msg {
            NodeListMsg::ApplyFilter(apply) => {
                if let Some(apply) = apply {
                    self.apply_filter.store(apply);
                } else {
                    self.apply_filter.fetch_xor(true);
                }
            }
            NodeListMsg::SetFiltered(nodes) => {
                self.set_filtered(&nodes);
            }
        }
    }

    pub fn new(
        graph_query: &GraphQuery,
        node_details_id: Arc<AtomicCell<Option<NodeId>>>,
    ) -> Self {
        let graph = graph_query.graph();

        let mut all_nodes = graph.handles().map(|h| h.id()).collect::<Vec<_>>();
        all_nodes.sort();

        let filtered_nodes: Vec<NodeId> = Vec::new();

        Self {
            all_nodes,
            filtered_nodes,

            apply_filter: true.into(),

            node_details_id,

            range: (0, 0).into(),

            col_widths: Default::default(),
        }
    }

    pub fn set_filtered(&mut self, nodes: &[NodeId]) {
        self.filtered_nodes.clear();
        self.filtered_nodes.extend(nodes.iter().copied());
    }

    pub fn ui(
        &mut self,
        ctx: &egui::CtxRef,
        app_msg_tx: &Sender<AppMsg>,
        open_node_details: &mut bool,
        graph_query: &GraphQuery,
        ctx_mgr: &ContextMgr,
    ) -> Option<egui::InnerResponse<Option<()>>> {
        let filter = self.apply_filter.load();

        let nodes = if !filter || self.filtered_nodes.is_empty() {
            &self.all_nodes
        } else {
            &self.filtered_nodes
        };
        egui::Window::new("Nodes")
            .id(egui::Id::new(Self::ID))
            .default_pos(egui::Pos2::new(200.0, 200.0))
            .show(ctx, |ui| {
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

                let apply_filter = &self.apply_filter;

                if ui.selectable_label(filter, "Show only selected").clicked() {
                    apply_filter.store(!filter);
                }

                let scroll_align = gui_util::add_scroll_buttons(ui);

                let node_id_cell = &self.node_details_id;

                let text_style = egui::TextStyle::Body;
                let row_height = ui.fonts()[text_style].row_height();
                let spacing = ui.style().spacing.item_spacing.y;

                let num_rows = nodes.len();

                let (start, end) = self.range.load();

                ui.label(format!(
                    "Showing {}-{} out of {} nodes",
                    start,
                    end,
                    nodes.len()
                ));

                let widths = self.col_widths.get();

                egui::Grid::new("node_list_grid_header").show(ui, |ui| {
                    let inner = grid_row_label(
                        ui,
                        egui::Id::new("node_list_grid_header__"),
                        &[
                            "Node",
                            "Degree",
                            "Seq. len",
                            "Unique paths",
                            "Total paths",
                        ],
                        false,
                        Some(&widths),
                    );

                    let ws = inner.inner;
                    self.col_widths.set_hdr(&ws);
                });

                gui_util::scrolled_area(ui, num_rows, scroll_align).show_rows(
                    ui,
                    row_height,
                    num_rows,
                    |ui, range| {
                        egui::Grid::new("node_list_grid").striped(true).show(
                            ui,
                            |ui| {
                                self.range.store((range.start, range.end));
                                let n =
                                    range.start.max(range.end) - range.start;

                                let graph = graph_query.graph();

                                for (ix, node_id) in nodes
                                    .iter()
                                    .copied()
                                    .enumerate()
                                    .skip(range.start)
                                    .take(n)
                                {
                                    let node_id_lb = format!("{}", node_id);
                                    let handle = Handle::pack(node_id, false);

                                    let deg_l =
                                        graph.degree(handle, Direction::Left);
                                    let deg_r =
                                        graph.degree(handle, Direction::Right);

                                    let degree =
                                        format!("({}, {})", deg_l, deg_r);

                                    let seq_len =
                                        format!("{}", graph.node_len(handle));

                                    let mut path_count = 0;
                                    let mut uniq_count = 0;

                                    let mut seen_paths: FxHashSet<PathId> =
                                        FxHashSet::default();

                                    if let Some(steps) =
                                        graph.steps_on_handle(handle)
                                    {
                                        for (path, _) in steps {
                                            if seen_paths.insert(path) {
                                                uniq_count += 1;
                                            }
                                            path_count += 1;
                                        }
                                    }

                                    let uniq_paths = format!("{}", uniq_count);

                                    let step_count = format!("{}", path_count);

                                    let fields: [&str; 5] = [
                                        &node_id_lb,
                                        &degree,
                                        &seq_len,
                                        &uniq_paths,
                                        &step_count,
                                    ];

                                    let inner = grid_row_label(
                                        ui,
                                        egui::Id::new(ui.id().with(ix)),
                                        &fields,
                                        false,
                                        Some(&widths),
                                    );

                                    self.col_widths.set(&inner.inner);

                                    let row = inner.response;

                                    if row.clicked() {
                                        node_id_cell.store(Some(node_id));

                                        *open_node_details = true;
                                    }

                                    if row.hovered() {
                                        ctx_mgr.produce_context(|| node_id);
                                    }

                                    /*
                                    if row.clicked_by(
                                        egui::PointerButton::Secondary,
                                    ) {
                                        ctx_tx
                                            .send(ContextEntry::Node(node_id))
                                            .unwrap();
                                    }
                                    */
                                }
                            },
                        );

                        /*
                        if let Some(align) = scroll_align {
                            // ui.scroll_to_cursor(align)
                            ui.scroll_to_cursor(align);
                        }
                        */
                    },
                );

                ui.shrink_width_to_current();
            })
    }
}
