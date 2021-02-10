use crate::geometry::*;
use crate::view::*;

use crossbeam::channel;

use std::sync::Arc;

use parking_lot::Mutex;

use std::thread;

pub struct UIThread {
    ui_state: Arc<Mutex<UIState>>,
    _ui_thread: thread::JoinHandle<()>,
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

            let mut last_time = std::time::Instant::now();

            let mut since_last_update = 0.0;

            loop {
                let delta = last_time.elapsed().as_secs_f32();
                last_time = std::time::Instant::now();
                since_last_update += delta;

                if since_last_update > 1.0 / 75.0 {
                    // if since_last_update > 0.0001 {
                    buf_ui_state.update_anim(since_last_update);
                    since_last_update = 0.0;
                }

                if let Ok(cmd) = rx_chan.try_recv() {
                    buf_ui_state.apply_cmd(cmd);
                }

                if let Some(mut ui_lock) = ui_state.try_lock() {
                    ui_lock.clone_from(&buf_ui_state);
                }
            }
        });

        let this = Self {
            ui_state,
            _ui_thread: handle,
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

#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct UIAnimState {
    view_delta: Point,
    scale_delta: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum UICmd {
    Pan { delta: Point },
    Zoom { delta: f32 },
    SetCenter { center: Point },
    SetScale { scale: f32 },
    Resize { width: f32, height: f32 },
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct UIState {
    anim: UIAnimState,
    pub view: View,
}

impl Default for UIState {
    fn default() -> Self {
        let view = View::default();

        Self {
            view,
            anim: Default::default(),
        }
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

        Self {
            view,
            anim: Default::default(),
        }
    }

    pub fn update_anim(&mut self, t: f32) {
        // println!("delta {}", t);
        // println!("fps {}", 1.0 / t);
        let zoom_friction = 0.999999;

        // let pan_friction = 1.0 - (1.0 + 0.5 * t).log2();
        // let pan_friction = 0.999999;
        let pan_friction = 1.0 - (0.5 * t);
        // let pan_friction = 1.0 - (0.75 * t);

        let dx = self.anim.view_delta.x;
        let dy = self.anim.view_delta.y;
        let dz = self.anim.scale_delta;

        self.view.scale += t * dz;
        // self.view.scale = self.view.scale.max(1.0);
        self.view.scale = self.view.scale.max(0.5);

        self.view.center.x += t * dx * self.view.scale;
        self.view.center.y += t * dy * self.view.scale;

        self.anim.view_delta *= pan_friction;
        self.anim.scale_delta *= zoom_friction;

        // if self.anim.view_delta.x.abs() < 0.01 {
        //     self.anim.view_delta.x = 0.0;
        // }

        // if self.anim.view_delta.y.abs() < 0.01 {
        //     self.anim.view_delta.y = 0.0;
        // }
    }

    pub fn apply_cmd(&mut self, cmd: UICmd) {
        match cmd {
            // UICmd::Idle => {}
            UICmd::Pan { delta } => {
                // self.view.center += delta;
                self.anim.view_delta += delta;

                let d = &mut self.anim.view_delta;

                let max_speed = 100.0;

                d.x = d.x.max(-max_speed).min(max_speed);
                d.y = d.y.max(-max_speed).min(max_speed);

                // if d.x < -max_speed {
                //     d.x = -max_speed;
                // } else if d.x > max_speed {
                //     d.x = max_speed;
                // }

                // if d.y < -max_speed {
                //     d.y = -max_speed;
                // } else if d.y > max_speed {
                //     d.y = max_speed;
                // }
            }
            UICmd::Zoom { delta } => {
                let delta_mult = self.view.scale.log2();
                let delta_mult = delta_mult.max(1.0);
                self.view.scale += delta * delta_mult;
            }
            UICmd::SetCenter { center } => {
                self.anim.view_delta = Point::default();
                self.anim.scale_delta = 0.0;
                self.view.center = center;
            }
            UICmd::SetScale { scale } => {
                self.anim.view_delta = Point::default();
                self.anim.scale_delta = 0.0;
                self.view.scale = scale;
            }
            UICmd::Resize { width, height } => {
                self.view.width = width;
                self.view.height = height;
            }
        }
    }
}
