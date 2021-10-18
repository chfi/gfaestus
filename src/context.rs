use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use clipboard::{ClipboardContext, ClipboardProvider};
use crossbeam::atomic::AtomicCell;
use crossbeam::channel;

use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use bstr::ByteSlice;
use rustc_hash::FxHashSet;

use crate::{
    app::{selection::NodeSelection, AppMsg},
    geometry::{Point, Rect},
    reactor::Reactor,
};

#[derive(Debug, Clone)]
pub enum ContextEntry {
    Node(NodeId),
    Path(PathId),
    Selection {
        // rect: Rect,
        nodes: FxHashSet<NodeId>,
    },
    // HasSelection,
    // HasSelection(bool),
}

// TODO this should be handled dynamically
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContextAction {
    CopyNodeId,
    CopyNodeSeq,
    CopyPathName,
    CopySubgraphGfa,
    // CopySelection,
    // CopyPathNames,
}

#[derive(Debug, Default, Clone)]
struct Contexts {
    node: Option<NodeId>,
    path: Option<PathId>,

    has_selection: bool,

    // selection_rect: Option<Rect>,
    selection_nodes: Option<FxHashSet<NodeId>>,
}

impl Contexts {
    fn is_not_empty(&self) -> bool {
        self.node.is_some() || self.path.is_some() || self.has_selection
    }
}

#[derive(Debug)]
pub struct ContextMenu {
    rx: channel::Receiver<ContextEntry>,
    tx: channel::Sender<ContextEntry>,

    position: Arc<AtomicCell<Point>>,

    contexts: Contexts,
    // contexts: Vec<ContextEntry>,
}

impl std::default::Default for ContextMenu {
    fn default() -> Self {
        let (tx, rx) = channel::unbounded();
        Self {
            tx,
            rx,
            position: Arc::new(Point::ZERO.into()),
            contexts: Default::default(),
        }
    }
}

impl ContextMenu {
    const ID: &'static str = "context_menu";

    const POPUP_ID: &'static str = "context_menu_popup_id";

    fn popup_id() -> egui::Id {
        egui::Id::new(Self::POPUP_ID)
    }

    pub fn tx(&self) -> &channel::Sender<ContextEntry> {
        &self.tx
    }

    pub fn recv_contexts(&mut self) {
        self.contexts = Default::default();

        // TODO add combining step, maybe?
        while let Ok(ctx) = self.rx.try_recv() {
            match ctx {
                ContextEntry::Node(node) => self.contexts.node = Some(node),
                ContextEntry::Path(path) => self.contexts.path = Some(path),
                ContextEntry::Selection { nodes } => {
                    self.contexts.selection_nodes = Some(nodes);
                    self.contexts.has_selection = true;
                }
            }
        }
    }

    fn process(
        &self,
        app_msg_tx: &channel::Sender<AppMsg>,
        reactor: &Reactor,
        clipboard: &mut ClipboardContext,
        action: ContextAction,
        contexts: &Contexts,
    ) {
        match action {
            ContextAction::CopyNodeId => {
                if let Some(node) = contexts.node {
                    let contents = node.0.to_string();
                    let _ = clipboard.set_contents(contents);
                }
            }
            ContextAction::CopyNodeSeq => {
                if let Some(node) = contexts.node {
                    let sequence = reactor
                        .graph_query
                        .graph
                        .sequence_vec(Handle::pack(node, false));

                    let contents = format!("{}", sequence.as_bstr());
                    let _ = clipboard.set_contents(contents);
                }
            }
            ContextAction::CopyPathName => {
                if let Some(path) = contexts.path {
                    if let Some(name) =
                        reactor.graph_query.graph.get_path_name_vec(path)
                    {
                        let contents = format!("{}", name.as_bstr());
                        let _ = clipboard.set_contents(contents);
                    }
                }
            }
            ContextAction::CopySubgraphGfa => {
                if let Some(nodes) = &contexts.selection_nodes {
                    let mut nodes = nodes.iter().copied().collect::<Vec<_>>();
                    nodes.sort();

                    let mut contents = String::new();

                    for node in &nodes {
                        let handle = Handle::pack(*node, false);
                        let sequence =
                            reactor.graph_query.graph.sequence_vec(handle);

                        contents.push_str(&format!(
                            "{}\t{}\n",
                            node.0,
                            sequence.as_bstr()
                        ));
                    }

                    /*
                    for node in &nodes {
                        let left = reactor
                            .graph_query
                            .graph
                            .neighbors(handle, Direction::Left);
                        let right = reactor
                            .graph_query
                            .graph
                            .neighbors(handle, Direction::Right);
                    }
                    */

                    let _ = clipboard.set_contents(contents);

                    log::warn!("selection has {} nodes", nodes.len());
                }
            }
        }
    }

    pub fn show(
        &self,
        egui_ctx: &egui::CtxRef,
        app_msg_tx: &channel::Sender<AppMsg>,
        reactor: &Reactor,
        clipboard: &mut ClipboardContext,
    ) {
        if egui_ctx.memory().is_popup_open(Self::popup_id()) {
            let screen_pos = self.position.load();

            let mut should_close = false;

            let popup_response = egui::Area::new(Self::ID)
                .order(egui::Order::Foreground)
                .fixed_pos(screen_pos)
                .show(egui_ctx, |ui| {
                    let frame = egui::Frame::popup(ui.style());
                    let frame_margin = frame.margin;
                    frame.show(ui, |ui| {
                        ui.with_layout(
                            egui::Layout::top_down_justified(egui::Align::LEFT),
                            |ui| {
                                if let Some(_node) = self.contexts.node {
                                    if ui.button("Copy node ID").clicked() {
                                        self.process(
                                            app_msg_tx,
                                            reactor,
                                            clipboard,
                                            ContextAction::CopyNodeId,
                                            &self.contexts,
                                        );
                                        should_close = true;
                                    }
                                    if ui.button("Copy node sequence").clicked()
                                    {
                                        self.process(
                                            app_msg_tx,
                                            reactor,
                                            clipboard,
                                            ContextAction::CopyNodeSeq,
                                            &self.contexts,
                                        );
                                        should_close = true;
                                    }
                                }

                                if let Some(_path) = self.contexts.path {
                                    if ui.button("Copy path name").clicked() {
                                        self.process(
                                            app_msg_tx,
                                            reactor,
                                            clipboard,
                                            ContextAction::CopyPathName,
                                            &self.contexts,
                                        );
                                        should_close = true;
                                    }
                                }

                                if self.contexts.has_selection {
                                    if ui
                                        .button("Copy subgraph as GFA")
                                        .clicked()
                                    {
                                        self.process(
                                            app_msg_tx,
                                            reactor,
                                            clipboard,
                                            ContextAction::CopySubgraphGfa,
                                            &self.contexts,
                                        );
                                        should_close = true;
                                    }
                                }
                            },
                        );
                    });
                });

            let popup_response = popup_response.response;

            if egui_ctx.input().key_pressed(egui::Key::Escape)
                || popup_response.clicked()
                || popup_response.clicked_elsewhere()
                || should_close
            {
                egui_ctx.memory().close_popup();
            }
        }
    }

    pub fn open_context_menu(&self, ctx: &egui::CtxRef) {
        if self.contexts.is_not_empty() {
            ctx.memory().open_popup(Self::popup_id());
        } else {
            // NB this might prove to be a problem later
            ctx.memory().close_popup()
        }
    }

    pub fn set_position(&self, pos: Point) {
        self.position.store(pos);
    }
}
