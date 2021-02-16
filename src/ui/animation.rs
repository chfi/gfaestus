#![allow(dead_code)]

use crate::geometry::*;
/*
use crate::view::*;

use std::sync::Arc;

use parking_lot::Mutex;
*/

use crossbeam::channel;

use std::thread;

pub struct AnimState {
    view_delta: Point,
    scale_delta: f32,
    // view: Point,
    // scale: f32,
}

pub enum AnimUpdate {
    SetPanAccel { accel: Point },
    SetPanVel { vel: Point },
    SetScaleAccel { accel: f32 },
    SetScaleVel { vel: Point },
    PanTo { target: Point },
    ScaleTo { target: f32 },
    PanScaleTo { center: Point, scale: f32 },
}

// pub struct AnimationChannels {
//     update_tx: channel::Sender<AnimUpdate>,
//     request_view_tx: channel::Sender<()>,
// }

pub struct AnimationWorker {
    anim_state: AnimState,
    _anim_thread: thread::JoinHandle<()>,
    update_rx: channel::Receiver<AnimUpdate>,
    update_tx: channel::Sender<AnimUpdate>,
    request_view_rx: channel::Receiver<()>,
    request_view_tx: channel::Sender<()>,
    view_rx: channel::Receiver<(Point, f32)>,
    view_tx: channel::Sender<(Point, f32)>,
}

/*
impl AnimationWorker {
    pub fn new() -> Self {
        let (update_tx, update_rx) = channel::unbounded::<AnimUpdate>();
        let (request_view_tx, request_view_rx) = channel::bounded::<()>(1);

        let anim_state = AnimState {
            view_delta: Point::new(0.0, 0.0),
            scale_delta: 0.0,
        };

        // let _anim_thread = thread::spawn(move || {
        //     let mut buf_anim_state =
        // });

        unimplemented!();
    }
}

*/
