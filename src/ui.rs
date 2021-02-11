use crate::geometry::*;
use crate::view::*;

use crossbeam::channel;

use std::sync::Arc;

use parking_lot::Mutex;

use std::thread;

pub mod animation;

pub mod events;

pub struct UIThread {
    _ui_thread: thread::JoinHandle<()>,
}

impl UIThread {
    pub fn new(width: f32, height: f32) -> (Self, channel::Sender<UICmd>, channel::Receiver<View>) {
        let (tx_chan, rx_chan) = channel::unbounded::<UICmd>();

        let (view_tx, view_rx) = channel::bounded::<View>(1);

        let mut ui_state = UIState::new(width, height);

        let handle = thread::spawn(move || {
            let mut last_time = std::time::Instant::now();

            let mut since_last_update = 0.0;

            loop {
                let delta = last_time.elapsed().as_secs_f32();
                since_last_update += delta;

                if since_last_update > 1.0 / 144.0 {
                    ui_state.update_anim(since_last_update);
                    since_last_update = 0.0;
                }

                last_time = std::time::Instant::now();

                if let Ok(cmd) = rx_chan.try_recv() {
                    ui_state.apply_cmd(cmd);
                }

                if view_tx.is_empty() {
                    view_tx.send(ui_state.view).unwrap();
                }
            }
        });

        let this = Self { _ui_thread: handle };

        (this, tx_chan, view_rx)
    }
}

#[derive(Default, Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct UIAnimState {
    mouse_pan_origin: Option<Point>,
    view_target: Option<Point>,
    view_const_delta: Point,
    view_delta: Point,
    scale_delta: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum UICmd {
    StartMousePan { origin: Point },
    MousePan { screen_tgt: Point },
    EndMousePan,
    PanConstant { delta: Point },
    Pan { delta: Point },
    Zoom { delta: f32 },
    SetCenter { center: Point },
    SetScale { scale: f32 },
    Resize { width: f32, height: f32 },
}

pub enum UIInputState {
    Mouse1Down,
    Mouse2Down,
    KeyUpDown,
    KeyRightDown,
    KeyDownDown,
    KeyLeftDown,
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
        let zoom_friction = 1.0 - (10.0_f32.powf(t - 1.0));
        let pan_friction = 1.0 - (10.0_f32.powf(t - 1.0));

        let mut dx = t * self.anim.view_delta.x + self.anim.view_const_delta.x;
        let mut dy = t * self.anim.view_delta.y + self.anim.view_const_delta.y;

        let dz = self.anim.scale_delta;

        self.view.scale += t * dz;
        self.view.scale = self.view.scale.max(0.5);

        if let Some(origin) = self.anim.mouse_pan_origin {
            if let Some(tgt) = self.anim.view_target {
                // use this one for continuous panning in the dragged direction
                dx = (tgt.x - origin.x) / 100.0;
                dy = (tgt.y - origin.y) / 100.0;

                // dx = (origin.x - tgt.x) / 100.0;
                // dy = (origin.y - tgt.y) / 100.0;

                // dx = (origin.x - tgt.x) / 10.0;
                // dy = (origin.y - tgt.y) / 10.0;

                // dx = (origin.x - tgt.x) / 1.0;
                // dy = (origin.y - tgt.y) / 1.0;

                // self.view.center.x = self.view.center.x + dx;
                // self.view.center.y = self.view.center.y + dy;

                let mut new_tgt = tgt;
                new_tgt.x += dx * self.view.scale;
                new_tgt.y += dy * self.view.scale;

                let mut new_origin = origin;
                new_origin.x -= dx * self.view.scale;
                new_origin.y -= dy * self.view.scale;

                let new_dist = new_origin.dist(new_tgt);

                // self.anim.mouse_pan_origin = Some(new_origin);
            }
        }

        self.view.center.x += dx * self.view.scale;
        self.view.center.y += dy * self.view.scale;

        // if let Some(tgt) = self.anim.view_target {

        // }

        self.anim.view_delta *= pan_friction;
        self.anim.scale_delta *= zoom_friction;

        if self.anim.scale_delta.abs() < 0.00001 {
            self.anim.scale_delta = 0.0;
        }
    }

    pub fn apply_cmd(&mut self, cmd: UICmd) {
        match cmd {
            // UICmd::Idle => {}
            UICmd::StartMousePan { origin } => {
                self.anim.mouse_pan_origin = Some(origin);
                self.anim.view_target = Some(origin);
                //
            }
            UICmd::MousePan { screen_tgt } => {
                if let Some(origin) = self.anim.mouse_pan_origin {
                    // self.anim.view_target = Some(origin + screen_tgt);
                    self.anim.view_target = Some(screen_tgt);
                    // if let Some(tgt) = self.anim.view_target {
                    //     self.anim.view_target = Some(tgt
                    // }
                    // let prev_tgt = self.anim.view_target.unwrap_or(origin);
                    // self.anim.view_target = Some(prev_tgt + delta);
                }
                // self.anim.mouse_pan_origin = Some(origin);
                //
            }
            UICmd::EndMousePan => {
                self.anim.mouse_pan_origin = None;
                self.anim.view_target = None;
                //
            }
            UICmd::PanConstant { delta } => {
                if delta.x == 0.0 {
                    self.anim.view_delta.x = self.anim.view_const_delta.x;
                }
                if delta.y == 0.0 {
                    self.anim.view_delta.y = self.anim.view_const_delta.y;
                }
                self.anim.view_const_delta = delta;
            }
            UICmd::Pan { delta } => {
                // self.view.center += delta;
                self.anim.view_delta += delta;

                let d = &mut self.anim.view_delta;

                let max_speed = 600.0;

                d.x = d.x.max(-max_speed).min(max_speed);
                d.y = d.y.max(-max_speed).min(max_speed);
            }
            UICmd::Zoom { delta } => {
                let delta_mult = self.view.scale.log2();
                let delta_mult = delta_mult.max(1.0);
                self.anim.scale_delta += delta * delta_mult;
                // self.view.scale += delta * delta_mult;
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
