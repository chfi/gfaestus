use crossbeam::channel::{self, Receiver, Sender};
use winit::event::VirtualKeyCode;

use crate::app::mainview::MainViewMsg;
use crate::app::AppMsg;
use crate::gui::GuiMsg;
use crate::overlays::OverlayData;

pub type BindMsg = (
    VirtualKeyCode,
    Option<Box<dyn Fn() + Send + Sync + 'static>>,
);

pub enum OverlayCreatorMsg {
    NewOverlay { name: String, data: OverlayData },
}

#[derive(Clone)]
pub struct AppChannels {
    pub app_tx: Sender<AppMsg>,
    pub app_rx: Receiver<AppMsg>,

    pub main_view_tx: Sender<MainViewMsg>,
    pub main_view_rx: Receiver<MainViewMsg>,

    pub gui_tx: Sender<GuiMsg>,
    pub gui_rx: Receiver<GuiMsg>,

    pub new_overlay_tx: Sender<OverlayCreatorMsg>,
    pub new_overlay_rx: Receiver<OverlayCreatorMsg>,

    pub modal_tx: Sender<Box<dyn Fn(&mut egui::Ui) + Send + Sync + 'static>>,
    pub modal_rx: Receiver<Box<dyn Fn(&mut egui::Ui) + Send + Sync + 'static>>,
}

impl AppChannels {
    pub(super) fn new() -> Self {
        let (app_tx, app_rx) = channel::unbounded::<AppMsg>();
        let (main_view_tx, main_view_rx) = channel::unbounded::<MainViewMsg>();
        let (gui_tx, gui_rx) = channel::unbounded::<GuiMsg>();
        let (binds_tx, binds_rx) = channel::unbounded::<BindMsg>();
        let (new_overlay_tx, new_overlay_rx) =
            channel::unbounded::<OverlayCreatorMsg>();

        let (modal_tx, modal_rx) = channel::unbounded();

        Self {
            app_tx,
            app_rx,

            main_view_tx,
            main_view_rx,

            gui_tx,
            gui_rx,

            new_overlay_tx,
            new_overlay_rx,

            modal_tx,
            modal_rx,
        }
    }
}
