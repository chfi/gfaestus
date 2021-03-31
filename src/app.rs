pub mod gui;
pub mod mainview;
pub mod settings;
pub mod theme;

use std::sync::Arc;

use crossbeam::channel;

use rustc_hash::{FxHashMap, FxHashSet};

use handlegraph::handle::NodeId;

use anyhow::Result;

use crate::input::binds::{BindableInput, InputPayload, KeyBind, SystemInput};
use crate::input::MousePos;
use crate::view::*;
use crate::{geometry::*, input::binds::SystemInputBindings};

use theme::*;

pub use settings::*;

pub struct App {
    // themes: Themes,
    mouse_pos: MousePos,
    screen_dims: ScreenDims,

    hover_node: Option<NodeId>,
    selected_nodes: FxHashSet<NodeId>,

    pub selection_edge_detect: bool,
    pub selection_edge_blur: bool,
    pub selection_edge: bool,
    pub nodes_color: bool,

    pub use_overlay: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AppInput {
    KeyClearSelection,
    // KeyToggleTheme,
    KeyToggleOverlay,
}

impl BindableInput for AppInput {
    fn default_binds() -> SystemInputBindings<Self> {
        use winit::event::VirtualKeyCode as Key;
        use AppInput as Input;

        let key_binds: FxHashMap<Key, Vec<KeyBind<Input>>> = [
            (Key::Escape, Input::KeyClearSelection),
            // (Key::F9, Input::KeyToggleTheme),
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
        // queue: Arc<Queue>,
        mouse_pos: MousePos,
        screen_dims: Dims,
    ) -> Result<Self> {
        // let themes = Themes::new_from_primary_and_secondary(
        //     queue.clone(),
        //     &dark_default(),
        //     &light_default(),
        // )?;

        Ok(Self {
            // themes,
            mouse_pos,
            screen_dims: screen_dims.into(),

            hover_node: None,
            selected_nodes: FxHashSet::default(),

            selection_edge_detect: true,
            selection_edge_blur: true,
            selection_edge: true,
            nodes_color: true,

            use_overlay: false,
        })
    }

    // pub fn themes(&self) -> &Themes {
    //     &self.themes
    // }

    // pub fn active_theme(&self) -> Option<(ThemeId, &Theme)> {
    //     self.themes.active_theme()
    // }

    // pub fn active_theme_ignore_cache(&self) -> (ThemeId, &Theme) {
    //     self.themes.active_theme_ignore_cache()
    // }

    // pub fn active_theme_def(&self) -> (ThemeId, &ThemeDef) {
    //     let (id, _) = self.themes.active_theme_ignore_cache();
    //     let def = self.themes.get_theme_def(id);
    //     (id, def)
    // }

    // pub fn all_theme_defs(&self) -> Vec<(ThemeId, &ThemeDef)> {
    //     let mut res = Vec::new();
    //     res.push((ThemeId::Primary, &self.themes.primary_def));
    //     res.push((ThemeId::Secondary, &self.themes.secondary_def));
    //     res
    // }

    // pub fn active_theme_luma(&self) -> f32 {
    //     let (_, theme) = self.active_theme_ignore_cache();
    //     theme.bg_luma()
    // }

    // pub fn dark_active_theme(&self) -> bool {
    //     let (_, theme) = self.active_theme_ignore_cache();
    //     theme.is_dark()
    // }

    // pub fn theme_upload_future(&mut self) -> Option<Box<dyn GpuFuture>> {
    //     self.themes.take_future()
    // }

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
            match payload {
                AppInput::KeyClearSelection => {
                    if state.pressed() {
                        self.selected_nodes.clear();
                    }
                }
                // AppInput::KeyToggleTheme => {
                //     if state.pressed() {
                //         let new_theme = self.themes.toggle_theme();
                //         let is_dark = self.dark_active_theme();
                //         let luma = self.active_theme_luma();
                //         println!(
                //             "{:?}\tdark? {}\tluma: {}",
                //             new_theme, is_dark, luma
                //         );
                //     }
                // }
                AppInput::KeyToggleOverlay => {
                    if state.pressed() {
                        self.use_overlay = !self.use_overlay;
                    }
                }
            }
        }
    }

    // pub fn active_theme_config_state(&self) -> settings::AppConfigState {
    //     let (id, _) = self.themes.active_theme_ignore_cache();

    //     let def = self.themes.get_theme_def(id).clone();

    //     settings::AppConfigState::Theme { id, def }
    // }

    pub fn apply_app_config_state(&mut self, app_cfg: AppConfigState) {
        match app_cfg {
            // AppConfigState::Theme { id, def } => {
            //     self.themes.replace_theme_def(id, def).unwrap();
            // }
            AppConfigState::ToggleOverlay => {
                self.use_overlay = !self.use_overlay;
            }
        }
    }
}
