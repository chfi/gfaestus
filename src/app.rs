pub mod gui;
pub mod mainview;
pub mod node_flags;

use node_flags::*;

use crossbeam::channel;

use rustc_hash::FxHashSet;

use handlegraph::handle::NodeId;

use crate::geometry::*;
use crate::input::binds::*;
use crate::input::MousePos;
use crate::view::*;

pub struct App {
    mouse_pos: MousePos,
    screen_dims: ScreenDims,

    hover_node: Option<NodeId>,
    selected_nodes: FxHashSet<NodeId>,

    pub selection_edge_detect: bool,
    pub selection_edge_blur: bool,
    pub selection_edge: bool,
    pub nodes_color: bool,
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
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppConfigMsg {
    ToggleSelectionEdgeDetect,
    ToggleSelectionEdgeBlur,
    ToggleSelectionOutline,
    ToggleNodesColor,
    // Toggle(RenderConfigOpts),
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
    ) -> Self {
        Self {
            mouse_pos,
            screen_dims: screen_dims.into(),

            hover_node: None,
            selected_nodes: FxHashSet::default(),

            selection_edge_detect: true,
            selection_edge_blur: true,
            selection_edge: true,
            nodes_color: true,
        }
    }

    pub fn hover_node(&self) -> Option<NodeId> {
        self.hover_node
    }

    pub fn selected_nodes(&self) -> Option<&FxHashSet<NodeId>> {
        if self.selected_nodes.is_empty() {
            None
        } else {
            Some(&self.selected_nodes)
        }
    }

    pub fn dims(&self) -> ScreenDims {
        self.screen_dims
    }

    pub fn mouse_pos(&self) -> Point {
        self.mouse_pos.read()
    }

    pub fn update_dims<Dims: Into<ScreenDims>>(&mut self, screen_dims: Dims) {
        self.screen_dims = screen_dims.into();
    }

    pub fn apply_app_msg(&mut self, msg: &AppMsg) {
        match msg {
            AppMsg::HoverNode(id) => self.hover_node = *id,
            AppMsg::Selection(sel) => match sel {
                Select::Clear => {
                    self.selected_nodes.clear();
                }
                Select::One { node, clear } => {
                    if *clear {
                        self.selected_nodes.clear();
                    }
                    self.selected_nodes.insert(*node);
                }
                Select::Many { nodes, clear } => {
                    if *clear {
                        self.selected_nodes.clear();
                    }
                    self.selected_nodes.extend(nodes.iter().copied());
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

    pub fn apply_input(&mut self, input: SystemInput<AppInput>) {
        if let SystemInput::Keyboard { state, payload } = input {
            if let AppInput::KeyClearSelection = payload {
                if state.pressed() {
                    self.selected_nodes.clear();
                }
            }
        }
    }
}
