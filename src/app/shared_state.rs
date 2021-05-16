use std::sync::Arc;

use crossbeam::atomic::AtomicCell;
use handlegraph::handle::NodeId;

use crate::view::*;
use crate::{geometry::*, input::binds::SystemInputBindings};
use crate::{gui::GuiMsg, input::MousePos};

#[derive(Clone)]
pub struct SharedState {
    pub(super) mouse_pos: MousePos,
    pub(super) screen_dims: Arc<AtomicCell<ScreenDims>>,

    pub(super) hover_node: Arc<AtomicCell<Option<NodeId>>>,

    pub(super) mouse_rect: MouseRect,
}

impl SharedState {
    pub fn new<Dims: Into<ScreenDims>>(
        mouse_pos: MousePos,
        screen_dims: Dims,
    ) -> Self {
        Self {
            mouse_pos,
            screen_dims: Arc::new(AtomicCell::new(screen_dims.into())),

            hover_node: Arc::new(AtomicCell::new(None)),

            mouse_rect: MouseRect::default(),
        }
    }
}

#[derive(Clone)]
pub struct MouseRect {
    pub(super) world_pos: Arc<AtomicCell<Option<Point>>>,
    pub(super) screen_pos: Arc<AtomicCell<Option<Point>>>,
}

impl std::default::Default for MouseRect {
    fn default() -> Self {
        Self {
            world_pos: Arc::new(AtomicCell::new(None)),
            screen_pos: Arc::new(AtomicCell::new(None)),
        }
    }
}
