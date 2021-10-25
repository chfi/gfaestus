use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use clipboard::{ClipboardContext, ClipboardProvider};
use crossbeam::atomic::AtomicCell;
use crossbeam::channel;

use futures::StreamExt;
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
    app::{selection::NodeSelection, App, AppChannels, AppMsg, SharedState},
    geometry::{Point, Rect},
    reactor::{ModalError, ModalHandler, ModalSuccess, Reactor},
};

#[derive(Debug, Clone)]
pub enum ContextEntry {
    Node(NodeId),
    Path(PathId),
    Selection {
        // rect: Rect,
        nodes: FxHashSet<NodeId>,
    },
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
    PanToNode,
}

#[derive(Debug, Default, Clone)]
struct Contexts {
    node: Option<NodeId>,
    path: Option<PathId>,

    has_selection: bool,

    selection_nodes: Option<FxHashSet<NodeId>>,
}

impl Contexts {
    fn is_not_empty(&self) -> bool {
        self.node.is_some() || self.path.is_some() || self.has_selection
    }
}

pub struct ContextMenu {
    rx: channel::Receiver<ContextEntry>,
    tx: channel::Sender<ContextEntry>,

    channels: AppChannels,
    shared_state: SharedState,
    position: Arc<AtomicCell<Point>>,

    contexts: Contexts,
}

impl ContextMenu {
    const ID: &'static str = "context_menu";

    const POPUP_ID: &'static str = "context_menu_popup_id";

    pub fn new(app: &App) -> Self {
        let (tx, rx) = channel::unbounded();

        // let modal_tx = channels.modal_tx.clone();
        let channels = app.channels().clone();
        let shared_state = app.shared_state().clone();

        Self {
            tx,
            rx,
            channels,
            shared_state,
            position: Arc::new(Point::ZERO.into()),
            contexts: Default::default(),
        }
    }

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
            ContextAction::PanToNode => {
                let (result_tx, mut result_rx) =
                    futures::channel::mpsc::channel::<Option<String>>(1);

                let first_run = AtomicCell::new(true);

                let callback = move |text: &mut String, ui: &mut egui::Ui| {
                    ui.label("Enter node ID");
                    let text_box = ui.text_edit_singleline(text);

                    if first_run.fetch_and(false) {
                        text_box.request_focus();
                    }

                    if text_box.lost_focus()
                        && ui.input().key_pressed(egui::Key::Enter)
                    {
                        return Ok(ModalSuccess::Success);
                    }

                    Err(ModalError::Continue)
                };

                let prepared = ModalHandler::prepare_callback(
                    &self.shared_state.show_modal,
                    String::new(),
                    callback,
                    result_tx,
                );

                self.channels.modal_tx.send(prepared).unwrap();

                let graph = reactor.graph_query.graph.clone();
                let app_tx = self.channels.app_tx.clone();

                std::thread::spawn(move || {
                    let value = futures::executor::block_on(async move {
                        result_rx.next().await
                    })
                    .flatten();

                    if let Some(parsed) =
                        value.and_then(|v| v.parse::<u64>().ok())
                    {
                        let node_id = NodeId::from(parsed);
                        if graph.has_node(node_id) {
                            app_tx.send(AppMsg::GotoNode(node_id)).unwrap();
                        }
                    }
                });
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
                                            reactor,
                                            clipboard,
                                            ContextAction::CopySubgraphGfa,
                                            &self.contexts,
                                        );
                                        should_close = true;
                                    }
                                }

                                if ui.button("Pan to node").clicked() {
                                    self.process(
                                        reactor,
                                        clipboard,
                                        ContextAction::PanToNode,
                                        &self.contexts,
                                    );
                                    should_close = true;
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
        ctx.memory().open_popup(Self::popup_id());
    }

    pub fn set_position(&self, pos: Point) {
        self.position.store(pos);
    }
}
