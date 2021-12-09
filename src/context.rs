use std::{
    any::TypeId,
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
use parking_lot::RwLock;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{
    app::{selection::NodeSelection, App, AppChannels, AppMsg, SharedState},
    geometry::{Point, Rect},
    reactor::{ModalError, ModalHandler, ModalSuccess, Reactor},
};

///////////////

pub struct OverGraph {}

#[derive(Default, Clone)]
pub struct Context {
    values: FxHashMap<TypeId, Arc<dyn std::any::Any + Send + Sync + 'static>>,
}

impl Context {
    fn get<T: std::any::Any + Send + Sync + 'static>(&self) -> Option<&Arc<T>> {
        let type_id = TypeId::of::<T>();
        let val = self.values.get(&type_id)?;
        val.downcast_ref()
    }

    fn get_raw<T: std::any::Any>(
        &self,
    ) -> Option<&Arc<dyn std::any::Any + Send + Sync + 'static>> {
        let type_id = TypeId::of::<T>();
        self.values.get(&type_id)
    }
}

#[derive(Clone)]
pub struct ContextAction_ {
    // name: Arc<String>,
    req: Arc<FxHashSet<TypeId>>,
    action: Arc<
        dyn Fn(&mut ClipboardContext, &App, &Context) + Send + Sync + 'static,
    >,
}

impl ContextAction_ {
    pub fn new(
        // name: &str,
        req: &[TypeId],
        action: impl Fn(&mut ClipboardContext, &App, &Context)
            + Send
            + Sync
            + 'static,
    ) -> Self {
        let req = Arc::new(req.iter().copied().collect());
        let action = Arc::new(action) as Arc<_>;

        Self {
            // name: name.to_string(),
            req,
            action,
        }
    }

    pub fn is_applicable(&self, ctx: &Context) -> bool {
        self.req.iter().all(|r| ctx.values.contains_key(r))
    }

    pub fn apply_action(
        &self,
        clipboard: &mut ClipboardContext,
        app: &App,
        ctx: &Context,
    ) -> Option<()> {
        if !self.is_applicable(ctx) {
            return None;
        }

        (self.action)(clipboard, app, ctx);

        Some(())
    }
}

pub type CtxVal = Arc<dyn std::any::Any + Send + Sync + 'static>;

pub struct ContextMgr {
    initialized: AtomicCell<bool>,

    load_context_this_frame: Arc<AtomicCell<bool>>,
    context_menu_open: Arc<AtomicCell<bool>>,

    pub ctx_tx: channel::Sender<(TypeId, CtxVal)>,
    ctx_rx: channel::Receiver<(TypeId, CtxVal)>,

    frame_context: AtomicCell<Arc<Context>>,
    frame_active: AtomicCell<bool>,

    // context_order: RwLock<Vec<String>>,
    context_actions: RwLock<HashMap<String, ContextAction_>>,

    position: Arc<AtomicCell<Point>>,
    // context_types: FxHashMap<String, TypeId>,
    // contexts: FxHashMap<
    //     TypeId,
    //     Option<Box<dyn std::any::Any + Send + Sync + 'static>>,
    // >,
    // context_actions: FxHashMap<String, ()>,
}

pub fn copy_node_id_action() -> ContextAction_ {
    let req = [TypeId::of::<NodeId>()];
    ContextAction_::new(&req, |clipboard, app, ctx| {
        let node_id = ctx.get::<NodeId>().unwrap();
        let contents = node_id.0.to_string();
        log::warn!("totally copying to clipboard here: {}", contents);
        // todo support clipboard access here
        // let _ = clipboard.set_contents(contents);
    })
}

pub fn pan_to_node_action() -> ContextAction_ {
    // let req = [TypeId::of::<OverGraph>()];
    let req = [];

    ContextAction_::new(&req, |clipboard, app: &App, ctx| {
        let (result_tx, mut result_rx) =
            futures::channel::mpsc::channel::<Option<String>>(1);

        let first_run = AtomicCell::new(true);

        let callback = move |text: &mut String, ui: &mut egui::Ui| {
            ui.label("Enter node ID");
            let text_box = ui.text_edit_singleline(text);

            if first_run.fetch_and(false) {
                text_box.request_focus();
            }

            if text_box.lost_focus() && ui.input().key_pressed(egui::Key::Enter)
            {
                return Ok(ModalSuccess::Success);
            }

            Err(ModalError::Continue)
        };

        let prepared = ModalHandler::prepare_callback(
            &app.shared_state.show_modal,
            String::new(),
            callback,
            result_tx,
        );

        app.channels.modal_tx.send(prepared).unwrap();

        let graph = app.reactor.graph_query.graph.clone();
        let app_tx = app.channels.app_tx.clone();

        app.reactor
            .spawn_forget(async move {
                let value = result_rx.next().await.flatten();

                if let Some(parsed) = value.and_then(|v| v.parse::<u64>().ok())
                {
                    let node_id = NodeId::from(parsed);
                    if graph.has_node(node_id) {
                        app_tx.send(AppMsg::goto_node(node_id)).unwrap();
                    }
                }
            })
            .unwrap();
    })
}

impl std::default::Default for ContextMgr {
    fn default() -> Self {
        let (ctx_tx, ctx_rx) = channel::unbounded();

        Self {
            initialized: false.into(),
            ctx_tx,
            ctx_rx,
            load_context_this_frame: Arc::new(false.into()),
            context_menu_open: Arc::new(false.into()),
            frame_context: Arc::new(Context::default()).into(),
            frame_active: false.into(),
            // context_order: RwLock::new(Vec::default()),
            context_actions: RwLock::new(HashMap::default()),
            position: Arc::new(Point::ZERO.into()),
        }
    }
}

impl ContextMgr {
    pub fn register_action(&self, name: &str, action: ContextAction_) {
        let mut actions = self.context_actions.write();

        if actions.insert(name.to_string(), action).is_some() {
            log::warn!("context action overwritten: {}", name);
        }
    }

    pub fn produce_context<T, F>(&self, prod: F)
    where
        T: std::any::Any + Send + Sync + 'static,
        F: FnOnce() -> T,
    {
        let type_id = TypeId::of::<T>();
        log::warn!("in produce_context for {:?}", type_id);
        if self.load_context_this_frame.load() {
            log::warn!("it's happening!!!");
            let value = prod();
            self.ctx_tx.send((type_id, Arc::new(value))).unwrap();
        }

        // if !self.initialized.load() {

        // }
    }

    pub fn open_context_menu(&self, ctx: &egui::CtxRef) {
        ctx.memory().open_popup(Self::popup_id());
        if !self.context_menu_open.load() {
            self.load_context_this_frame.store(true);
        }
        self.context_menu_open.store(true);
    }

    pub fn set_position(&self, pos: Point) {
        self.position.store(pos);
    }

    pub fn close_context_menu(&self) {
        self.load_context_this_frame.store(false);
        self.context_menu_open.store(false);
    }

    pub fn begin_frame(&self) {
        // if !self.initialized.load() {
        //     self.initialized.store(false);
        //     self.frame_active.store(true);
        //     return;
        // }

        if self.frame_active.load() {
            log::error!("ContextMgr::begin_frame() has already been called before end_frame()");

            self.end_frame();
        }

        let mut context = Context::default();

        while let Ok((type_id, ctx_val)) = self.ctx_rx.try_recv() {
            context.values.insert(type_id, ctx_val);
        }

        self.frame_context.store(Arc::new(context));
        self.frame_active.store(true);
    }

    pub fn frame_context(&self) -> Option<Arc<Context>> {
        if self.frame_active.load() {
            let ctx = self.frame_context.take();
            self.frame_context.store(ctx.clone());
            Some(ctx)
        } else {
            None
        }
    }

    pub fn end_frame(&self) {
        if self.frame_active.load() {
            self.frame_context.take();
            self.frame_active.store(false);
        } else {
            log::error!(
                "ContextMgr::end_frame() was called before begin_frame()"
            );
        }
    }

    const ID: &'static str = "context_menu";

    const POPUP_ID: &'static str = "context_menu_popup_id";

    fn popup_id() -> egui::Id {
        egui::Id::new(Self::POPUP_ID)
    }

    pub fn show(
        &self,
        egui_ctx: &egui::CtxRef,
        app: &App,
        clipboard: &mut ClipboardContext,
    ) {
        if !self.frame_active.load() {
            log::error!("call begin_frame() before show()");
        }

        if egui_ctx.memory().is_popup_open(Self::popup_id()) {
            let screen_pos = self.position.load();

            let should_close = AtomicCell::new(false);

            /*
            let mut process = |action: ContextAction| {
                // self.process(reactor, clipboard, action, &self.contexts);
                should_close.store(true);
            };
            */

            let popup_response = egui::Area::new(Self::ID)
                .order(egui::Order::Foreground)
                .fixed_pos(screen_pos)
                .show(egui_ctx, |ui| {
                    let frame = egui::Frame::popup(ui.style());
                    frame.show(ui, |ui| {
                        let actions = self.context_actions.read();

                        let context = self.frame_context.take();

                        for (name, action) in actions.iter() {
                            if action.is_applicable(&context) {
                                if ui.button(name).clicked() {
                                    action
                                        .apply_action(clipboard, app, &context);
                                }
                            }
                        }
                    });
                });

            let popup_response = popup_response.response;

            if egui_ctx.input().key_pressed(egui::Key::Escape)
                || popup_response.clicked()
                || popup_response.clicked_elsewhere()
                || should_close.load()
            {
                egui_ctx.memory().close_popup();
            }
        }
    }
}

/*
pub struct ContextMgr {
    ctx_tx: channel::Sender<()>,   // each frame all contexts are sent here
    ctx_rx: channel::Receiver<()>, //

    frame_contexts: Vec<()>,
}

pub struct Context {
    context_type: TypeId,
}
*/

///////////////

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

                reactor
                    .spawn_forget(async move {
                        let value = result_rx.next().await.flatten();

                        if let Some(parsed) =
                            value.and_then(|v| v.parse::<u64>().ok())
                        {
                            let node_id = NodeId::from(parsed);
                            if graph.has_node(node_id) {
                                app_tx
                                    .send(AppMsg::goto_node(node_id))
                                    .unwrap();
                            }
                        }
                    })
                    .unwrap();
            }
        }
    }

    pub fn show(
        &self,
        egui_ctx: &egui::CtxRef,
        reactor: &Reactor,
        clipboard: &mut ClipboardContext,
    ) {
        if egui_ctx.memory().is_popup_open(Self::popup_id()) {
            let screen_pos = self.position.load();

            let should_close = AtomicCell::new(false);

            let mut process = |action: ContextAction| {
                self.process(reactor, clipboard, action, &self.contexts);
                should_close.store(true);
            };

            let popup_response = egui::Area::new(Self::ID)
                .order(egui::Order::Foreground)
                .fixed_pos(screen_pos)
                .show(egui_ctx, |ui| {
                    let frame = egui::Frame::popup(ui.style());
                    frame.show(ui, |ui| {
                        if let Some(_node) = self.contexts.node {
                            if ui.button("Copy node ID").clicked() {
                                process(ContextAction::CopyNodeId);
                            }
                            if ui.button("Copy node sequence").clicked() {
                                process(ContextAction::CopyNodeSeq);
                            }
                        }

                        if let Some(_path) = self.contexts.path {
                            if ui.button("Copy path name").clicked() {
                                process(ContextAction::CopyPathName);
                            }
                        }

                        if self.contexts.has_selection {
                            if ui.button("Copy subgraph as GFA").clicked() {
                                process(ContextAction::CopySubgraphGfa);
                            }
                        }

                        if ui.button("Pan to node").clicked() {
                            process(ContextAction::PanToNode);
                        }
                    });
                });

            let popup_response = popup_response.response;

            if egui_ctx.input().key_pressed(egui::Key::Escape)
                || popup_response.clicked()
                || popup_response.clicked_elsewhere()
                || should_close.load()
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
