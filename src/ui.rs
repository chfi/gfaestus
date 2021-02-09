// pub mod view;

use crate::geometry::*;
use crate::view::*;

use crossbeam::atomic::AtomicCell;
use crossbeam::channel;

use std::sync::Arc;

use parking_lot::Mutex;

use std::thread;

/*
pub struct UIThread {
    state: UIState,
    recv_cmd: channel::Receiver<UIAnim>,
}

impl UIThread {
    pub fn new(width: f32, height: f32, recv_cmd: channel::Receiver<UIAnim>) -> Self {
        let state = UIState::new(width, height);
        Self { state, recv_cmd }
    }
}
*/

pub struct UIThread {
    ui_state: Arc<Mutex<UIState>>,
    ui_thread: thread::JoinHandle<()>,
}

impl UIThread {
    pub fn new(width: f32, height: f32) -> (Self, channel::Sender<UICmd>) {
        let (tx_chan, rx_chan) = channel::unbounded::<UICmd>();

        let buf_ui_state = UIState::new(width, height);
        let ui_state = Arc::new(Mutex::new(buf_ui_state.clone()));

        let ui_state_inner = Arc::clone(&ui_state);

        let handle = thread::spawn(move || {
            let ui_state = ui_state_inner;
            let mut buf_ui_state = buf_ui_state.clone();

            while let Ok(cmd) = rx_chan.recv() {
                buf_ui_state.apply_cmd(cmd);

                if let Some(mut ui_lock) = ui_state.try_lock() {
                    ui_lock.clone_from(&buf_ui_state);
                }
            }
        });

        let this = Self {
            ui_state,
            ui_thread: handle,
        };

        (this, tx_chan)
    }

    pub fn try_get_state(&self) -> Option<UIState> {
        if let Some(ui_lock) = self.ui_state.try_lock() {
            let ui_state: UIState = ui_lock.clone();
            Some(ui_state)
        } else {
            None
        }
    }

    pub fn lock_get_state(&self) -> UIState {
        let ui_lock = self.ui_state.lock();
        let ui_state: UIState = ui_lock.clone();
        ui_state
    }
}

/*
pub fn ui_thread(width: f32, height: f32) -> (thread::JoinHandle<()>, channel::Sender<UICmd>) {
    let (tx_chan, rx_chan) = channel::unbounded::<UICmd>();

    let handle = thread::spawn(move || {
        let mut ui_state = UIState::new(width, height);
        while let Ok(cmd) = rx_chan.recv() {
            ui_state.apply_cmd(cmd);
        }
    });

    (handle, tx_chan)
}
*/

// #[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
// pub enum UICmd {
//     Anim(UIAnim),
//     Resize
// }

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum UICmd {
    // Idle,
    Pan { delta: Point },
    Zoom { delta: f32 },
    SetCenter { center: Point },
    SetScale { scale: f32 },
    Resize { width: f32, height: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct UIState {
    // anim: UICmd,
    view: View,
    // cmd_chan:
}

impl Default for UIState {
    fn default() -> Self {
        let view = View::default();
        // let anim = UICmd::Idle;
        // Self { view, anim }
        Self { view }
    }
}

impl UIState {
    pub fn new(width: f32, height: f32) -> Self {
        let view = View {
            center: Point::new(0.0, 0.0),
            scale: 1.0,
            width,
            height,
        };

        // let anim = UICmd::Idle;

        // Self { view, anim }
        Self { view }
    }

    pub fn apply_cmd(&mut self, cmd: UICmd) {
        match cmd {
            // UICmd::Idle => {}
            UICmd::Pan { delta } => {
                self.view.center += delta;
            }
            UICmd::Zoom { delta } => {
                self.view.scale += delta;
                self.view.scale = self.view.scale.max(1.0);
            }
            UICmd::SetCenter { center } => {
                self.view.center = center;
            }
            UICmd::SetScale { scale } => {
                self.view.scale = scale;
            }
            UICmd::Resize { width, height } => {
                self.view.width = width;
                self.view.height = height;
            }
        }
    }
}
