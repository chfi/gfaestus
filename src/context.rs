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

use crate::{geometry::Point, reactor::Reactor};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContextEntry {
    Node(NodeId),
    Path(PathId),
    HasSelection,
    // HasSelection(bool),
}

#[derive(Debug, Default, Clone, Copy)]
struct Contexts {
    node: Option<NodeId>,
    path: Option<PathId>,

    has_selection: bool,
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
                ContextEntry::HasSelection => {
                    self.contexts.has_selection = true
                }
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
                                if let Some(node) = self.contexts.node {
                                    if ui.button("Copy node ID").clicked() {
                                        let contents = node.0.to_string();
                                        let _ =
                                            clipboard.set_contents(contents);

                                        should_close = true;
                                    }
                                    if ui.button("Copy node sequence").clicked()
                                    {
                                        let sequence = reactor
                                            .graph_query
                                            .graph
                                            .sequence_vec(Handle::pack(
                                                node, false,
                                            ));

                                        let contents =
                                            format!("{}", sequence.as_bstr());
                                        let _ =
                                            clipboard.set_contents(contents);

                                        should_close = true;
                                    }
                                }

                                if let Some(path) = self.contexts.path {
                                    ui.label(&format!("Path {:?}", path));
                                }

                                if self.contexts.has_selection {
                                    ui.label("has selection");
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
