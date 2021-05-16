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

    pub(super) view: Arc<AtomicCell<View>>,

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

            view: Arc::new(AtomicCell::new(View::default())),

            hover_node: Arc::new(AtomicCell::new(None)),

            mouse_rect: MouseRect::default(),
        }
    }

    pub fn mouse_pos(&self) -> Point {
        self.mouse_pos.read()
    }

    pub fn screen_dims(&self) -> ScreenDims {
        self.screen_dims.load()
    }

    pub fn view(&self) -> View {
        self.view.load()
    }

    pub fn hover_node(&self) -> Option<NodeId> {
        self.hover_node.load()
    }

    pub fn clone_view(&self) -> Arc<AtomicCell<View>> {
        self.view.clone()
    }

    pub fn set_view(&self, view: View) {
        self.view.store(view)
    }

    // pub fn clone_mouse_rect(&self) -> MouseRect {
    //     self.mouse_rect.clone()
    // }

    pub fn start_mouse_rect(&self) {
        let view = self.view();
        let screen_pos = self.mouse_pos();
        let screen_dims = self.screen_dims();

        let world_pos = view.screen_point_to_world(screen_dims, screen_pos);

        self.mouse_rect.screen_pos.store(Some(screen_pos));
        self.mouse_rect.world_pos.store(Some(world_pos));
    }

    pub fn active_mouse_rect_screen(&self) -> Option<Rect> {
        let start_pos = self.mouse_rect.screen_pos.load()?;

        let end_pos = self.mouse_pos();

        Some(Rect::new(start_pos, end_pos))
    }

    pub fn close_mouse_rect_world(&self) -> Option<Rect> {
        let start_pos = self.mouse_rect.world_pos.load()?;

        let screen_pos = self.mouse_pos();
        let screen_dims = self.screen_dims();

        let view = self.view();

        let end_pos = view.screen_point_to_world(screen_dims, screen_pos);

        let rect = Rect::new(start_pos, end_pos);

        self.mouse_rect.world_pos.store(None);
        self.mouse_rect.screen_pos.store(None);

        Some(rect)
    }

    pub fn is_started_mouse_rect(&self) -> bool {
        self.mouse_rect.screen_pos.load().is_some()
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
