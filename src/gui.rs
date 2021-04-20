use std::sync::Arc;

#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use anyhow::Result;

use rustc_hash::FxHashMap;

use crossbeam::channel;
use parking_lot::Mutex;

// mod theme_editor;

// use theme_editor::*;

use crate::geometry::*;
use crate::view::View;
use crate::{app::RenderConfigOpts, vulkan::render_pass::Framebuffers};

use crate::graph_query::GraphQuery;

use crate::input::binds::{
    BindableInput, InputPayload, KeyBind, MouseButtonBind, SystemInput,
    SystemInputBindings, WheelBind,
};
use crate::input::MousePos;

// use super::theme::{ThemeDef, ThemeId};

use crate::app::settings::AppConfigState;

use crate::vulkan::{
    context::VkContext,
    draw_system::gui::{GuiPipeline, GuiVertex, GuiVertices},
    GfaestusVk, SwapchainProperties,
};

use ash::vk;

pub mod widgets;
pub mod windows;

use widgets::*;
use windows::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Windows {
    Settings,

    // Console,
    FPS,
    GraphStats,
    // ViewInfo,
    Nodes,
    Paths,

    Themes,
    Overlays,
}

pub struct ViewStateChannel<T, U>
where
    U: Send + Sync,
{
    state: T,
    tx: crossbeam::channel::Sender<U>,
    rx: crossbeam::channel::Receiver<U>,
}

impl<T, U> std::default::Default for ViewStateChannel<T, U>
where
    T: Default,
    U: Send + Sync,
{
    fn default() -> Self {
        let (tx, rx) = crossbeam::channel::unbounded::<U>();
        let state = T::default();

        Self { state, tx, rx }
    }
}

impl<T, U> ViewStateChannel<T, U>
where
    U: Send + Sync,
{
    pub fn new(state: T) -> Self {
        let (tx, rx) = crossbeam::channel::unbounded::<U>();
        Self { state, tx, rx }
    }

    pub fn send(&self, msg: U) {
        self.tx.send(msg).unwrap();
    }

    pub fn apply_received<F>(&mut self, f: F)
    where
        F: for<'a> Fn(&'a mut T, U),
    {
        while let Ok(msg) = self.rx.try_recv() {
            f(&mut self.state, msg);
        }
    }
}

pub struct AppViewState {
    // settings: (),
    fps: ViewStateChannel<FrameRate, FrameRateMsg>,

    graph_stats: ViewStateChannel<GraphStats, GraphStatsMsg>,

    // view_info: ViewStateChannel<ViewInfo, ViewInfoMsg>,
    node_list: ViewStateChannel<NodeList, NodeListMsg>,
    // node_details: NodeList,
    path_list: ViewStateChannel<NodeList, NodeListMsg>,
    // path_details: PathList,

    // theme_editor: ThemeEditor,
    // theme_list: ThemeList,

    // overlay_editor: OverlayEditor,
    // overlay_list: OverlayList,
}

impl AppViewState {
    pub fn new(graph_query: &GraphQuery) -> Self {
        // let fps = ViewStateChannel::<FrameRate, FrameRateMsg>::default();

        let graph = graph_query.graph();

        let stats = GraphStats {
            node_count: graph.node_count(),
            edge_count: graph.edge_count(),
            path_count: graph.path_count(),
            total_len: graph.total_length(),
        };

        let node_list_state = NodeList::new(graph_query, 15);

        let path_list_state = NodeList::new(graph_query, 15);

        let node_list =
            ViewStateChannel::<NodeList, NodeListMsg>::new(node_list_state);
        let path_list =
            ViewStateChannel::<NodeList, NodeListMsg>::new(path_list_state);

        Self {
            fps: Default::default(),
            graph_stats: ViewStateChannel::new(stats),

            node_list,

            path_list,
        }
    }

    pub fn apply_received(&mut self) {
        self.fps.apply_received(|state, msg| {
            *state = FrameRate::apply_msg(state, msg);
        });

        self.graph_stats.apply_received(|state, msg| {
            *state = GraphStats::apply_msg(state, msg);
        });

        self.node_list.apply_received(|state, msg| {
            state.apply_msg(msg);
        });

        self.path_list.apply_received(|state, msg| {
            state.apply_msg(msg);
        });
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Views {
    Settings,

    // Console,
    FPS,
    GraphStats,

    NodeList,
    NodeDetails,

    PathList,
    PathDetails,

    ThemeEditor,
    ThemeList,

    OverlayEditor,
    OverlayList,
}

impl Windows {
    pub fn name(&self) -> &str {
        match self {
            Windows::Settings => "Settings",
            Windows::FPS => "FPS",
            Windows::GraphStats => "Graph Stats",
            Windows::Nodes => "Nodes",
            Windows::Paths => "Paths",
            Windows::Themes => "Themes",
            Windows::Overlays => "Overlays",
        }
    }

    pub fn views(&self) -> Vec<Views> {
        match self {
            Windows::Settings => vec![Views::Settings],
            Windows::FPS => vec![Views::FPS],
            Windows::GraphStats => vec![Views::GraphStats],
            Windows::Nodes => vec![Views::NodeList, Views::NodeDetails],
            Windows::Paths => vec![Views::PathList, Views::PathDetails],
            Windows::Themes => vec![Views::ThemeEditor, Views::ThemeList],
            Windows::Overlays => vec![Views::OverlayEditor, Views::OverlayList],
        }
    }

    pub fn all_windows() -> [Windows; 7] {
        [
            Self::Settings,
            Self::FPS,
            Self::GraphStats,
            Self::Nodes,
            Self::Paths,
            Self::Themes,
            Self::Overlays,
        ]
    }
}

impl Views {
    pub fn window(&self) -> Windows {
        match self {
            Views::Settings => Windows::Settings,
            Views::FPS => Windows::FPS,
            Views::GraphStats => Windows::GraphStats,
            Views::NodeList | Views::NodeDetails => Windows::Nodes,
            Views::PathList | Views::PathDetails => Windows::Paths,
            Views::ThemeEditor | Views::ThemeList => Windows::Themes,
            Views::OverlayEditor | Views::OverlayList => Windows::Overlays,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct OpenWindows {
    settings: bool,

    fps: bool,
    graph_stats: bool,

    nodes: bool,
    paths: bool,

    themes: bool,
    overlays: bool,
}

impl std::default::Default for OpenWindows {
    fn default() -> Self {
        Self {
            settings: false,
            fps: true,
            graph_stats: true,
            nodes: true,
            paths: false,
            themes: false,
            overlays: false,
        }
    }
}

pub struct Gui {
    ctx: egui::CtxRef,
    frame_input: FrameInput,

    draw_system: GuiPipeline,

    hover_node_id: Option<NodeId>,

    windows_active_view: FxHashMap<Windows, Views>,

    open_windows: OpenWindows,
    // widgets: FxHashMap<String,

    // windows:
    // details_win: NodeList,
    // theme_editor_win: ThemeEditor,
}

impl Gui {
    pub fn new(
        app: &GfaestusVk,
        graph_query: &GraphQuery,
        swapchain_props: SwapchainProperties,
        msaa_samples: vk::SampleCountFlags,
        render_pass: vk::RenderPass,
    ) -> Result<(Self, channel::Receiver<AppConfigState>)> {
        let draw_system = GuiPipeline::new(app, msaa_samples, render_pass)?;

        let ctx = egui::CtxRef::default();

        let mut style: egui::Style = (*ctx.style()).clone();
        style.visuals.window_corner_radius = 0.0;
        ctx.set_style(style);

        let font_defs = {
            use egui::FontFamily as Family;
            use egui::TextStyle as Style;

            let mut font_defs = egui::FontDefinitions::default();
            let fam_size = &mut font_defs.family_and_size;

            fam_size.insert(Style::Small, (Family::Proportional, 12.0));
            fam_size.insert(Style::Body, (Family::Proportional, 16.0));
            fam_size.insert(Style::Button, (Family::Proportional, 18.0));
            fam_size.insert(Style::Heading, (Family::Proportional, 22.0));
            font_defs
        };
        ctx.set_fonts(font_defs);

        let hover_node_id: Option<NodeId> = None;

        let windows = Windows::all_windows();

        let windows_active_view = {
            windows
                .iter()
                .copied()
                .filter_map(|w| w.views().first().copied().map(|v| (w, v)))
                .collect::<FxHashMap<_, _>>()
        };

        let open_windows = OpenWindows::default();

        let frame_input = FrameInput::default();

        let (sender, receiver) = channel::unbounded::<AppConfigState>();

        let gui = Self {
            ctx,
            frame_input,

            draw_system,

            hover_node_id,

            windows_active_view,

            open_windows,
        };

        Ok((gui, receiver))
    }


    pub fn active_views(&self) -> Vec<Views> {
        let mut views: Vec<_> =
            self.windows_active_view.values().copied().collect();
        views.sort();
        views
    }
}

struct ActiveWindows {
    egui_inspection_ui: bool,
    egui_settings_ui: bool,
    egui_memory_ui: bool,

    graph_info: bool,

    selection_info: bool,

    theme_editor: bool,

    options: bool,
}

struct ActiveWidgets {
    fps: bool,
    graph_stats: bool,
    view_info: bool,
    selected_node: bool,
}

/// Wrapper for input events that are fed into egui
#[derive(Debug, Default, Clone)]
struct FrameInput {
    events: Vec<egui::Event>,
    scroll_delta: f32,
}

impl FrameInput {
    fn into_raw_input(&mut self) -> egui::RawInput {
        let mut raw_input = egui::RawInput::default();
        // TODO maybe use clone_from and clear self.events instead, to reduce allocations
        raw_input.events = std::mem::take(&mut self.events);
        raw_input.scroll_delta = egui::Vec2 {
            x: 0.0,
            y: self.scroll_delta,
        };
        self.scroll_delta = 0.0;

        raw_input
    }
}
