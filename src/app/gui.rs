use std::sync::Arc;

#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

use anyhow::{Context, Result};

use rgb::*;
use vulkano::{
    command_buffer::{AutoCommandBuffer, DynamicState},
    device::Queue,
    framebuffer::{RenderPassAbstract, Subpass},
    sync::GpuFuture,
};

use crate::geometry::*;
use crate::gfa::*;
use crate::input::*;
// use crate::render::*;
use crate::render::GuiDrawSystem;
use crate::view;
use crate::view::View;

pub struct GfaestusGui {
    ctx: egui::CtxRef,
    events: Vec<egui::Event>,
    hover_node_id: Option<NodeId>,
    selected_node_id: Option<NodeId>,
    gui_draw_system: GuiDrawSystem,
    graph_stats: GraphStatsUi,
    view_info: ViewInfoUi,
}

#[derive(Debug, Default, Clone, Copy)]
struct ViewInfoUi {
    enabled: bool,
    position: Point,
    view: View,
    mouse_screen: Point,
    mouse_world: Point,
}

#[derive(Debug, Default, Clone, Copy)]
struct GraphStatsUi {
    position: Point,
    enabled: bool,
    stats: GraphStats,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GraphStats {
    pub node_count: usize,
    pub edge_count: usize,
    pub path_count: usize,
    pub total_len: usize,
}

impl GfaestusGui {
    pub fn new<R>(gfx_queue: Arc<Queue>, render_pass: &Arc<R>) -> Result<GfaestusGui>
    where
        R: RenderPassAbstract + Send + Sync + 'static,
    {
        let gui_draw_system =
            GuiDrawSystem::new(gfx_queue, Subpass::from(render_pass.clone(), 0).unwrap());

        let ctx = egui::CtxRef::default();

        let mut style: egui::Style = (*ctx.style()).clone();
        style.visuals.window_corner_radius = 0.0;
        ctx.set_style(style);

        let font_defs = {
            let mut font_defs = egui::FontDefinitions::default();
            let fam_size = &mut font_defs.family_and_size;
            fam_size.insert(
                egui::TextStyle::Small,
                (egui::FontFamily::Proportional, 12.0),
            );
            fam_size.insert(
                egui::TextStyle::Body,
                (egui::FontFamily::Proportional, 16.0),
            );
            fam_size.insert(
                egui::TextStyle::Button,
                (egui::FontFamily::Proportional, 18.0),
            );
            fam_size.insert(
                egui::TextStyle::Heading,
                (egui::FontFamily::Proportional, 22.0),
            );
            font_defs
        };
        ctx.set_fonts(font_defs);

        let events: Vec<egui::Event> = Vec::new();

        let hover_node_id = None;
        let selected_node_id = None;

        let graph_stats = GraphStatsUi {
            position: Point { x: 12.0, y: 20.0 },
            enabled: true,
            ..GraphStatsUi::default()
        };

        let view_info = ViewInfoUi {
            position: Point { x: 12.0, y: 140.0 },
            enabled: true,
            ..ViewInfoUi::default()
        };

        Ok(Self {
            ctx,
            events,
            hover_node_id,
            selected_node_id,
            gui_draw_system,
            graph_stats,
            view_info,
        })
    }

    pub fn set_graph_stats(&mut self, stats: GraphStats) {
        self.graph_stats.stats = stats;
    }

    pub fn set_view_info_view(&mut self, view: View) {
        self.view_info.view = view;
    }

    pub fn set_view_info_mouse(&mut self, mouse_screen: Point, mouse_world: Point) {
        self.view_info.mouse_screen = mouse_screen;
        self.view_info.mouse_world = mouse_world;
    }

    pub fn set_hover_node(&mut self, node: Option<NodeId>) {
        self.hover_node_id = node;
    }

    pub fn set_selected_node(&mut self, node: Option<NodeId>) {
        self.selected_node_id = node;
    }

    fn graph_stats(&self, at: Point) {
        let pos = egui::pos2(at.x, at.y);
        let stats = self.graph_stats.stats;

        egui::Area::new("graph_summary_stats")
            .fixed_pos(pos)
            .show(&self.ctx, |ui| {
                ui.label(format!("nodes: {}", stats.node_count));
                ui.label(format!("edges: {}", stats.edge_count));
                ui.label(format!("paths: {}", stats.path_count));
                ui.label(format!("total length: {}", stats.total_len));
            });
    }

    fn view_info(&self, at: Point) {
        let pos = egui::pos2(at.x, at.y);
        let info = self.view_info;

        egui::Area::new("view_mouse_info")
            .fixed_pos(pos)
            .show(&self.ctx, |ui| {
                ui.label(format!(
                    "view origin: x: {:6}\ty: {:6}",
                    info.view.center.x, info.view.center.y
                ));
                ui.label(format!("view scale: {}", info.view.scale));
                ui.label(format!(
                    "mouse world:  {:6}\t{:6}",
                    info.mouse_world.x, info.mouse_world.y
                ));
                ui.label(format!(
                    "mouse screen: {:6}\t{:6}",
                    info.mouse_screen.x, info.mouse_screen.y
                ));
            });
    }

    pub fn begin_frame(&mut self, screen_rect: Option<Point>) {
        let mut raw_input = egui::RawInput::default();
        let screen_rect = screen_rect.map(|p| egui::Rect {
            min: egui::Pos2 { x: 0.0, y: 0.0 },
            max: egui::Pos2 { x: p.x, y: p.y },
        });
        raw_input.screen_rect = screen_rect;
        raw_input.events = std::mem::take(&mut self.events);

        self.ctx.begin_frame(raw_input);

        let scr = self.ctx.input().screen_rect();

        if let Some(node_id) = self.hover_node_id {
            egui::containers::popup::show_tooltip_text(&self.ctx, node_id.0.to_string())
        }

        if let Some(node_id) = self.selected_node_id {
            let top_left = egui::Pos2 {
                x: 0.0,
                y: 0.80 * scr.max.y,
            };
            let bottom_right = egui::Pos2 {
                x: 200.0,
                y: scr.max.y,
            };

            let rect = egui::Rect {
                min: top_left,
                max: bottom_right,
            };

            egui::Window::new("node_select_info")
                .fixed_rect(rect)
                .title_bar(false)
                .show(&self.ctx, |ui| {
                    ui.expand_to_include_rect(egui::Rect {
                        min: top_left,
                        max: egui::Pos2 {
                            x: bottom_right.x,
                            y: bottom_right.y,
                            // y: scr.max.y - 5.0,
                        },
                    });
                    let label = format!("Selected: {}", node_id.0);
                    ui.label(label);
                });
        }

        if self.graph_stats.enabled {
            self.graph_stats(self.graph_stats.position);
        }

        if self.view_info.enabled {
            self.view_info(self.view_info.position);
        }

        {
            let mouse_egui = self.ctx.is_pointer_over_area();
            let p0 = egui::Pos2 {
                x: 0.8 * scr.max.x,
                y: 0.0,
            };

            let p1 = egui::Pos2 {
                x: scr.max.x,
                y: 80.0,
            };

            egui::Window::new("mouse_over_egui")
                .fixed_rect(egui::Rect { min: p0, max: p1 })
                .title_bar(false)
                .show(&self.ctx, |ui| {
                    if mouse_egui {
                        ui.label("Mouse is over egui");
                    } else {
                        ui.label("Mouse outside egui");
                    }
                });
        }
    }

    pub fn pointer_over_gui(&self) -> bool {
        self.ctx.is_pointer_over_area()
    }

    fn draw_tessellated(
        &mut self,
        dynamic_state: &DynamicState,
        clipped_meshes: &[egui::ClippedMesh],
    ) -> Result<(AutoCommandBuffer, Option<Box<dyn GpuFuture>>)> {
        let egui_tex = self.ctx.texture();
        let tex_future = self.gui_draw_system.upload_texture(&egui_tex).transpose()?;
        let cmd_buf = self
            .gui_draw_system
            .draw_egui_ctx(dynamic_state, clipped_meshes)?;

        Ok((cmd_buf, tex_future))
    }

    pub fn push_event(&mut self, event: egui::Event) {
        self.events.push(event);
    }

    pub fn end_frame_and_draw(
        &mut self,
        dynamic_state: &DynamicState,
    ) -> Option<Result<(AutoCommandBuffer, Option<Box<dyn GpuFuture>>)>> {
        let (output, shapes) = self.ctx.end_frame();
        let clipped_meshes = self.ctx.tessellate(shapes);

        if clipped_meshes.is_empty() {
            return None;
        }

        Some(self.draw_tessellated(dynamic_state, &clipped_meshes))
    }
}

// struct GraphViewActiveSet {
//     node_tooltip: bool,
//     node_info: bool,
//     graph_summary_stats: bool,
//     view_info: bool,
// }

/*
#[derive(Debug, Default, Clone, PartialEq)]
struct NodeInfo {
    // node_id: Option<NodeId>,
    sequence: Vec<u8>,
    paths: Vec<PathId>,
    neighbors: Vec<NodeId>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct GraphSummaryStats {
    node_count: usize,
    edge_count: usize,
    path_count: usize,
    total_len: usize,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct GraphView {
    node_hover: Option<NodeId>,
    node_selected: Option<NodeId>,
    node_selected_info: NodeInfo,
    graph_summary_stats: GraphSummaryStats,
    frame: usize,
    mouse_pos: Point,
}
*/

/*
#[derive(Debug, Default, Clone, Copy, PartialEq)]
struct NodeHoverTooltip {
    node_id: Option<NodeId>,
    offset: Point,
}

#[derive(Debug, Default, Clone, PartialEq)]
struct NodeInfoBox {
    node_id: Option<NodeId>,
    sequence: Vec<u8>,
    paths: Vec<Vec<u8>>,
    neighbors: Vec<NodeId>,
}

#[derive(Default, Clone, PartialEq)]
pub struct GfaestusGui {
    ctx: egui::CtxRef,
    events: Vec<egui::Event>,
    // raw_input
    // ref to PackedGraph?
    node_hover: NodeHoverTooltip,
    node_info_box: NodeInfoBox,

    mouse_pos: Point,
    window_dims: Point,
}
*/

/*

impl GfaestusGui {
    // pub fn set_screen_rect(&mut self, width: f32, height: f32) {
    // }
    pub fn add_event(&mut self, event: egui::Event) {
        self.events.push(event);
    }

    pub fn begin_frame(&mut self, screen_dims: Option<Point>) {
        let mut raw_input = egui::RawInput::default();

        let screen_rect = screen_dims.map(|p| egui::Rect {
            min: egui::Pos2 { x: 0.0, y: 0.0 },
            max: egui::Pos2 { x: p.x, y: p.y },
        });

        raw_input.screen_rect = screen_rect;
        raw_input.events = std::mem::take(&mut self.events);

        self.ctx.begin_frame(raw_input);

        if let Some(node_id) = self.node_hover.node_id.as_ref() {
            let pos = egui::pos2(
                (self.mouse_pos.x - 32.0).max(0.0).min(width),
                (self.mouse_pos.y - 24.0).max(0.0).min(height),
            );

            egui::Area::new("node_hover_tooltip")
                .fixed_pos(pos)
                .show(&egui_ctx, |ui| {
                    ui.label(node_id.0.to_string());
                });
        }
    }
}
*/
