use crossbeam::channel::{self, Receiver, Sender};

use crate::app::mainview::MainViewMsg;
use crate::app::AppMsg;
use crate::gui::GuiMsg;

#[derive(Clone)]
pub struct AppChannels {
    pub app_tx: Sender<AppMsg>,
    pub app_rx: Receiver<AppMsg>,

    pub main_view_tx: Sender<MainViewMsg>,
    pub main_view_rx: Receiver<MainViewMsg>,

    pub gui_tx: Sender<GuiMsg>,
    pub gui_rx: Receiver<GuiMsg>,
}

impl AppChannels {
    pub(super) fn new() -> Self {
        let (app_tx, app_rx) = channel::unbounded::<AppMsg>();
        let (main_view_tx, main_view_rx) = channel::unbounded::<MainViewMsg>();
        let (gui_tx, gui_rx) = channel::unbounded::<GuiMsg>();

        Self {
            app_tx,
            app_rx,

            main_view_tx,
            main_view_rx,

            gui_tx,
            gui_rx,
        }
    }
}
