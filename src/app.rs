pub mod gui;
pub mod mainview;
pub mod node_flags;
pub mod theme;

use node_flags::*;

use vulkano::device::Queue;
use vulkano::sync::GpuFuture;

use std::sync::Arc;

use crossbeam::channel;

use rustc_hash::FxHashSet;

use handlegraph::handle::NodeId;

use anyhow::Result;

use crate::geometry::*;
use crate::input::binds::*;
use crate::input::MousePos;
use crate::view::*;

use theme::*;

pub struct App {
    themes: Themes,

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
    ToggleLightDarkTheme,
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
        queue: Arc<Queue>,
        mouse_pos: MousePos,
        screen_dims: Dims,
    ) -> Result<Self> {
        let themes = Themes::new_from_light_and_dark(
            queue.clone(),
            &light_default(),
            &dark_default(),
        )?;

        Ok(Self {
            themes,

            mouse_pos,
            screen_dims: screen_dims.into(),

            hover_node: None,
            selected_nodes: FxHashSet::default(),

            selection_edge_detect: true,
            selection_edge_blur: true,
            selection_edge: true,
            nodes_color: true,
        })
    }

    pub fn themes(&self) -> &Themes {
        &self.themes
    }

    pub fn active_theme(&self) -> Option<(ThemeId, &Theme)> {
        self.themes.active_theme()
    }

    pub fn active_theme_ignore_cache(&self) -> (ThemeId, &Theme) {
        self.themes.active_theme_ignore_cache()
    }

    pub fn active_theme_luma(&self) -> f32 {
        let (_, theme) = self.active_theme_ignore_cache();
        theme.bg_luma()
    }

    pub fn dark_active_theme(&self) -> bool {
        let (_, theme) = self.active_theme_ignore_cache();
        theme.is_dark()
    }

    pub fn theme_upload_future(&mut self) -> Option<Box<dyn GpuFuture>> {
        self.themes.take_future()
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
            AppConfigMsg::ToggleLightDarkTheme => {
                self.themes.toggle_light_dark();
            }
        }
    }

    pub fn apply_input(&mut self, input: SystemInput<AppInput>) {
        if let SystemInput::Keyboard { state, payload } = input {
            match payload {
                AppInput::KeyClearSelection => {
                    if state.pressed() {
                        self.selected_nodes.clear();
                    }
                }
                AppInput::KeyToggleTheme => {
                    if state.pressed() {
                        let new_theme = self.themes.toggle_light_dark();
                        let is_dark = self.dark_active_theme();
                        let luma = self.active_theme_luma();
                        println!(
                            "{:?}\tdark? {}\tluma: {}",
                            new_theme, is_dark, luma
                        );
                    }
                }
            }
        }
    }
}
