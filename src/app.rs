pub mod mainview;
pub mod node_flags;
pub mod settings;
pub mod shared_state;
pub mod theme;

use crossbeam::{atomic::AtomicCell, channel::Sender};
use std::sync::Arc;

use crossbeam::channel;

use rustc_hash::{FxHashMap, FxHashSet};

use handlegraph::handle::NodeId;

use anyhow::Result;

use crate::view::*;
use crate::{geometry::*, input::binds::SystemInputBindings};
use crate::{gui::GuiMsg, input::MousePos};
use crate::{
    input::binds::{BindableInput, InputPayload, KeyBind, SystemInput},
    universe::Node,
};

use theme::*;

pub use settings::*;
pub use shared_state::*;

use self::mainview::MainViewMsg;

pub struct App {
    pub themes: AppThemes,

    shared_state: SharedState,

    selected_nodes: FxHashSet<NodeId>,
    selection_changed: bool,

    pub selected_nodes_bounding_box: Option<(Point, Point)>,

    pub selection_edge_detect: bool,
    pub selection_edge_blur: bool,
    pub selection_edge: bool,
    pub nodes_color: bool,

    pub overlay_state: OverlayState,

    pub settings: AppSettings,
}

#[derive(Debug, Clone)]
pub struct OverlayState {
    use_overlay: Arc<AtomicCell<bool>>,
    current_overlay: Arc<AtomicCell<Option<usize>>>,
}

impl OverlayState {
    pub fn use_overlay(&self) -> bool {
        self.use_overlay.load()
    }

    pub fn current_overlay(&self) -> Option<usize> {
        self.current_overlay.load()
    }

    pub fn set_use_overlay(&self, use_overlay: bool) {
        self.use_overlay.store(use_overlay);
    }

    pub fn toggle_overlay(&self) {
        self.use_overlay.fetch_xor(true);
    }

    pub fn set_current_overlay(&self, overlay_id: Option<usize>) {
        self.current_overlay.store(overlay_id);
    }
}

impl std::default::Default for OverlayState {
    fn default() -> Self {
        let use_overlay = Arc::new(AtomicCell::new(false));
        let current_overlay = Arc::new(AtomicCell::new(None));

        Self {
            use_overlay,
            current_overlay,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AppInput {
    KeyClearSelection,
    KeyToggleTheme,
    KeyToggleOverlay,
}

impl BindableInput for AppInput {
    fn default_binds() -> SystemInputBindings<Self> {
        use winit::event::VirtualKeyCode as Key;
        use AppInput as Input;

        let key_binds: FxHashMap<Key, Vec<KeyBind<Input>>> = [
            (Key::Escape, Input::KeyClearSelection),
            (Key::F9, Input::KeyToggleTheme),
            (Key::F10, Input::KeyToggleOverlay),
        ]
        .iter()
        .copied()
        .map(|(k, i)| (k, vec![KeyBind::new(i)]))
        .collect::<FxHashMap<_, _>>();

        let mouse_binds = FxHashMap::default();

        let wheel_bind = None;

        SystemInputBindings::new(key_binds, mouse_binds, wheel_bind)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Select {
    Clear,
    One {
        node: NodeId,
        clear: bool,
    },
    Many {
        nodes: FxHashSet<NodeId>,
        clear: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum AppMsg {
    Selection(Select),
    HoverNode(Option<NodeId>),
    GotoSelection,
    RectSelect(Rect),
    TranslateSelected(Point),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppConfigMsg {
    ToggleSelectionEdgeDetect,
    ToggleSelectionEdgeBlur,
    ToggleSelectionOutline,
    ToggleNodesColor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RenderConfigOpts {
    SelOutlineEdge,
    SelOutlineBlur,
    SelOutline,
    NodesColor,
}

impl App {
    pub fn new<Dims: Into<ScreenDims>>(
        mouse_pos: MousePos,
        screen_dims: Dims,
    ) -> Result<Self> {
        let themes = AppThemes::default_themes();

        let shared_state = SharedState::new(mouse_pos, screen_dims);

        Ok(Self {
            themes,

            shared_state,

            selected_nodes: FxHashSet::default(),
            selection_changed: false,

            selected_nodes_bounding_box: None,

            selection_edge_detect: true,
            selection_edge_blur: true,
            selection_edge: true,
            nodes_color: true,

            overlay_state: OverlayState::default(),

            settings: AppSettings::default(),
        })
    }

    pub fn shared_state(&self) -> &SharedState {
        &self.shared_state
    }

    pub fn hover_node(&self) -> Option<NodeId> {
        self.shared_state.hover_node.load()
    }

    pub fn selection_changed(&self) -> bool {
        self.selection_changed
    }

    pub fn selected_nodes(&mut self) -> Option<&FxHashSet<NodeId>> {
        if self.selected_nodes.is_empty() {
            self.selection_changed = false;
            None
        } else {
            self.selection_changed = false;
            Some(&self.selected_nodes)
        }
    }

    pub fn dims(&self) -> ScreenDims {
        self.shared_state.screen_dims.load()
    }

    pub fn mouse_pos(&self) -> Point {
        self.shared_state.mouse_pos.read()
    }

    pub fn update_dims<Dims: Into<ScreenDims>>(&mut self, screen_dims: Dims) {
        self.shared_state.screen_dims.store(screen_dims.into());
    }

    pub fn apply_app_msg(
        &mut self,
        main_view_msg_tx: &Sender<MainViewMsg>,
        msg: &AppMsg,
        node_positions: &[Node],
    ) {
        match msg {
            AppMsg::RectSelect(rect) => {
                //
            }
            AppMsg::TranslateSelected(delta) => {
                if let Some(bounds) = self.selected_nodes_bounding_box {
                    let min = bounds.0 + delta;
                    let max = bounds.1 + delta;

                    self.selected_nodes_bounding_box = Some((min, max));
                }
            }
            AppMsg::GotoSelection => {
                if let Some(bounds) = self.selected_nodes_bounding_box {
                    let view = View::from_dims_and_target(
                        self.dims(),
                        bounds.0,
                        bounds.1,
                    );
                    main_view_msg_tx.send(MainViewMsg::GotoView(view)).unwrap();
                }
            }
            AppMsg::HoverNode(id) => self.shared_state.hover_node.store(*id),
            AppMsg::Selection(sel) => match sel {
                Select::Clear => {
                    self.selection_changed = true;
                    self.selected_nodes.clear();
                    self.selected_nodes_bounding_box = None;
                }
                Select::One { node, clear } => {
                    self.selection_changed = true;
                    if *clear {
                        self.selected_nodes.clear();
                        self.selected_nodes_bounding_box = None;
                    }
                    self.selected_nodes.insert(*node);

                    let node_pos = node_positions[(node.0 - 1) as usize];

                    if let Some(bounds) = self.selected_nodes_bounding_box {
                        let old_min = Point {
                            x: bounds.0.x.min(bounds.1.x),
                            y: bounds.0.y.min(bounds.1.y),
                        };

                        let old_max = Point {
                            x: bounds.0.x.max(bounds.1.x),
                            y: bounds.0.y.max(bounds.1.y),
                        };

                        let top_left = Point {
                            x: old_min.x.min(node_pos.p0.x.min(node_pos.p1.x)),
                            y: old_min.y.min(node_pos.p0.y.min(node_pos.p1.y)),
                        };

                        let bottom_right = Point {
                            x: old_max.x.max(node_pos.p0.x.max(node_pos.p1.x)),
                            y: old_max.y.max(node_pos.p0.y.max(node_pos.p1.y)),
                        };

                        self.selected_nodes_bounding_box =
                            Some((top_left, bottom_right));
                    } else {
                        let top_left = Point {
                            x: node_pos.p0.x.min(node_pos.p1.x),
                            y: node_pos.p0.y.min(node_pos.p1.y),
                        };

                        let bottom_right = Point {
                            x: node_pos.p0.x.max(node_pos.p1.x),
                            y: node_pos.p0.y.max(node_pos.p1.y),
                        };

                        self.selected_nodes_bounding_box =
                            Some((top_left, bottom_right));
                    }
                }
                Select::Many { nodes, clear } => {
                    self.selection_changed = true;
                    if *clear {
                        self.selected_nodes.clear();
                        self.selected_nodes_bounding_box = None;
                    }
                    if self.selected_nodes.capacity() < nodes.len() {
                        let additional =
                            nodes.len() - self.selected_nodes.capacity();
                        self.selected_nodes.reserve(additional);
                    }

                    let (mut top_left, mut bottom_right) = if let Some(bounds) =
                        self.selected_nodes_bounding_box
                    {
                        let old_min = Point {
                            x: bounds.0.x.min(bounds.1.x),
                            y: bounds.0.y.min(bounds.1.y),
                        };

                        let old_max = Point {
                            x: bounds.0.x.max(bounds.1.x),
                            y: bounds.0.y.max(bounds.1.y),
                        };

                        (old_min, old_max)
                    } else {
                        let top_left = Point {
                            x: std::f32::MAX,
                            y: std::f32::MAX,
                        };

                        let bottom_right = Point {
                            x: std::f32::MIN,
                            y: std::f32::MIN,
                        };

                        (top_left, bottom_right)
                    };

                    for &node in nodes.iter() {
                        let pos = node_positions[(node.0 - 1) as usize];

                        let min_x = pos.p0.x.min(pos.p1.x);
                        let min_y = pos.p0.y.min(pos.p1.y);

                        let max_x = pos.p0.x.max(pos.p1.x);
                        let max_y = pos.p0.y.max(pos.p1.y);

                        top_left.x = top_left.x.min(min_x);
                        top_left.y = top_left.y.min(min_y);

                        bottom_right.x = bottom_right.x.max(max_x);
                        bottom_right.y = bottom_right.y.max(max_y);

                        self.selected_nodes.insert(node);
                    }

                    self.selected_nodes_bounding_box =
                        Some((top_left, bottom_right));
                }
            },
        }
    }

    pub fn apply_app_config_msg(&mut self, msg: &AppConfigMsg) {
        match msg {
            AppConfigMsg::ToggleSelectionEdgeDetect => {
                self.selection_edge_detect = !self.selection_edge_detect
            }
            AppConfigMsg::ToggleSelectionEdgeBlur => {
                self.selection_edge_blur = !self.selection_edge_blur
            }
            AppConfigMsg::ToggleSelectionOutline => {
                self.selection_edge = !self.selection_edge
            }
            AppConfigMsg::ToggleNodesColor => {
                self.nodes_color = !self.nodes_color
            }
        }
    }

    pub fn apply_input(
        &mut self,
        input: SystemInput<AppInput>,
        gui_msg: &Sender<GuiMsg>,
    ) {
        if let SystemInput::Keyboard { state, payload } = input {
            match payload {
                AppInput::KeyClearSelection => {
                    if state.pressed() {
                        self.selection_changed = true;
                        self.selected_nodes.clear();
                        self.selected_nodes_bounding_box = None;
                    }
                }
                AppInput::KeyToggleTheme => {
                    if state.pressed() {
                        self.themes.toggle_previous_theme();

                        let msg = if self.themes.is_active_theme_dark() {
                            GuiMsg::SetDarkMode
                        } else {
                            GuiMsg::SetLightMode
                        };

                        gui_msg.send(msg).unwrap();
                    }
                }
                AppInput::KeyToggleOverlay => {
                    if state.pressed() {
                        self.overlay_state.toggle_overlay();
                    }
                }
            }
        }
    }

    pub fn apply_app_config_state(&mut self, app_cfg: AppConfigState) {
        match app_cfg {
            // AppConfigState::Theme { id, def } => {
            //     self.themes.replace_theme_def(id, def).unwrap();
            // }
            AppConfigState::ToggleOverlay => {
                self.overlay_state.toggle_overlay();
            }
        }
    }
}
