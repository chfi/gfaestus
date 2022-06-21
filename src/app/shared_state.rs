use std::sync::Arc;

use crossbeam::atomic::AtomicCell;
use handlegraph::handle::NodeId;
use handlegraph::pathhandlegraph::PathId;

use crate::{geometry::*, gui::GuiFocusState};
use crate::{view::*, vulkan::texture::GradientName};

#[derive(Clone)]
pub struct SharedState {
    pub mouse_pos: Arc<AtomicCell<Point>>,
    pub screen_dims: Arc<AtomicCell<ScreenDims>>,

    pub view: Arc<AtomicCell<View>>,

    pub hover_node: Arc<AtomicCell<Option<NodeId>>>,

    pub mouse_rect: MouseRect,

    pub overlay_state: OverlayState,

    pub gui_focus_state: GuiFocusState,

    pub edges_enabled: Arc<AtomicCell<bool>>,

    pub dark_mode: Arc<AtomicCell<bool>>,

    pub show_modal: Arc<AtomicCell<bool>>,
}

impl SharedState {
    pub fn new<Dims: Into<ScreenDims>>(screen_dims: Dims) -> Self {
        Self {
            mouse_pos: Arc::new(Point::ZERO.into()),
            screen_dims: Arc::new(screen_dims.into().into()),

            view: Arc::new(View::default().into()),

            hover_node: Arc::new(None.into()),

            mouse_rect: MouseRect::default(),

            overlay_state: OverlayState::default(),

            gui_focus_state: GuiFocusState::default(),

            edges_enabled: Arc::new(true.into()),
            dark_mode: Arc::new(false.into()),
            show_modal: Arc::new(false.into()),
        }
    }

    pub fn mouse_pos(&self) -> Point {
        self.mouse_pos.load()
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

    pub fn overlay_state(&self) -> &OverlayState {
        &self.overlay_state
    }

    pub fn edges_enabled(&self) -> bool {
        self.edges_enabled.load()
    }

    pub fn dark_mode(&self) -> &Arc<AtomicCell<bool>> {
        &self.dark_mode
    }

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

#[derive(Debug, Clone)]
pub struct OverlayState {
    pub current_overlay: Arc<AtomicCell<Option<usize>>>,

    gradient: Arc<AtomicCell<GradientName>>,
}

impl OverlayState {
    pub fn current_overlay(&self) -> Option<usize> {
        self.current_overlay.load()
    }

    pub fn gradient(&self) -> GradientName {
        self.gradient.load()
    }

    pub fn set_current_overlay(&self, overlay_id: Option<usize>) {
        self.current_overlay.store(overlay_id);
    }

    pub fn set_gradient(&self, gradient: GradientName) {
        self.gradient.store(gradient);
    }
}

impl std::default::Default for OverlayState {
    fn default() -> Self {
        let current_overlay = Arc::new(AtomicCell::new(None));

        let gradient = Arc::new(AtomicCell::new(GradientName::Magma));

        Self {
            current_overlay,
            gradient,
        }
    }
}
