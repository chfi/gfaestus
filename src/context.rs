use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
};

use crossbeam::atomic::AtomicCell;
use crossbeam::channel;

use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ContextEntry {
    Node(NodeId),
    Path(PathId),
    HasSelection,
    // HasSelection(bool),
}

#[derive(Debug)]
pub struct ContextMenu {
    rx: channel::Receiver<ContextEntry>,
    tx: channel::Sender<ContextEntry>,
}

impl std::default::Default for ContextMenu {
    fn default() -> Self {
        let (tx, rx) = channel::unbounded();
        Self { tx, rx }
    }
}

impl ContextMenu {
    const ID: &'static str = "context_menu";

    pub fn tx(&self) -> &channel::Sender<ContextEntry> {
        &self.tx
    }

    pub fn show(&self, egui_ctx: &egui::CtxRef) {
        let mut contexts: HashSet<ContextEntry> = HashSet::default();

        // TODO add combining step, maybe?
        while let Ok(ctx) = self.rx.try_recv() {
            contexts.insert(ctx);
        }

        // TODO use a popup
        egui::Window::new(Self::ID).show(egui_ctx, |ui| {
            // todo actually do the thing
            for ctx in contexts {
                ui.label(&format!("{:?}", ctx));
            }
        });
    }
}
