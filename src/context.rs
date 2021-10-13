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

use crate::geometry::Point;

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

    position: Arc<AtomicCell<Point>>,
}

impl std::default::Default for ContextMenu {
    fn default() -> Self {
        let (tx, rx) = channel::unbounded();
        Self {
            tx,
            rx,
            position: Arc::new(Point::ZERO.into()),
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

    pub fn show_at(&self, egui_ctx: &egui::CtxRef, screen_pos: Point) {
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

    pub fn open_context_menu(&self, ctx: &egui::CtxRef) {
        log::warn!("opening context menu");
        ctx.memory().open_popup(Self::popup_id());
    }

    pub fn set_position(&self, pos: Point) {
        self.position.store(pos);
    }

    pub fn show_test(&self, egui_ctx: &egui::CtxRef) {
        if egui_ctx.memory().is_popup_open(Self::popup_id()) {
            let screen_pos = self.position.load();

            let popup_response = egui::Area::new(Self::ID)
                .order(egui::Order::Foreground)
                .fixed_pos(screen_pos)
                .show(egui_ctx, |ui| {
                    // ui.set_clip_rect(parent_clip_rect); // for when the combo-box is in a scroll area.
                    let frame = egui::Frame::popup(ui.style());
                    let frame_margin = frame.margin;
                    frame.show(ui, |ui| {
                        ui.with_layout(
                            egui::Layout::top_down_justified(egui::Align::LEFT),
                            |ui| {
                                /*
                                ui.set_width(
                                    widget_response.rect.width()
                                        - 2.0 * frame_margin.x,
                                );
                                add_contents(ui)
                                */
                                ui.label("hello contexts");
                            },
                        );
                    });
                });
        }
        // else {
        //     log::warn!("context menu CLOSED");
        // }
    }
}
