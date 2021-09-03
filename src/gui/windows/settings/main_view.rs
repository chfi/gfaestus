use crossbeam::atomic::AtomicCell;
use std::sync::Arc;

use crate::{
    app::{AppSettings, NodeWidth},
    vulkan::draw_system::edges::EdgesUBO,
};

pub struct MainViewSettings {
    node_width: Arc<NodeWidth>,
    label_radius: Arc<AtomicCell<f32>>,

    edges_enabled: Arc<AtomicCell<bool>>,
    edges_ubo: Arc<AtomicCell<EdgesUBO>>,
}

impl MainViewSettings {
    pub fn new(
        settings: &AppSettings,
        edges_enabled: Arc<AtomicCell<bool>>,
    ) -> Self {
        let node_width = settings.node_width().clone();
        let label_radius = settings.label_radius().clone();

        let edges_ubo = settings.edge_renderer().clone();

        Self {
            node_width,
            label_radius,

            edges_enabled,
            edges_ubo,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let mut min_width = self.node_width.min_node_width();
        let mut max_width = self.node_width.max_node_width();

        let mut min_scale = self.node_width.min_node_scale();
        let mut max_scale = self.node_width.max_node_scale();

        let min_node_width_slider = ui
            .add(
                egui::Slider::new::<f32>(&mut min_width, 0.1..=max_width)
                    .text("Min node width"),
            )
            .on_hover_text(
                "The minimum node width, in pixels at scale 1.0. Default: 0.1",
            );

        let max_node_width_slider = ui.add(
                    egui::Slider::new::<f32>(&mut max_width, min_width..=300.0).text("Max node width"),
                ).on_hover_text("The maximum node width, in pixels at scale 1.0. Default: 100.0");

        let min_scale_slider = ui.add(
                    egui::Slider::new::<f32>(&mut min_scale, 1.0..=max_scale).text("Min node width scale"),
                ).on_hover_text("The scale below which the minimum node width will be used. Default: 0.1");

        let max_scale_slider = ui.add(
                    egui::Slider::new::<f32>(&mut max_scale, min_scale..=1000.0).text("Max node width scale"),
                ).on_hover_text("The scale above which the maximum node width will be used. Default: 200.0");

        let edges_enabled = self.edges_enabled.load();
        let edges_button = ui.selectable_label(edges_enabled, "Show Edges");

        let mut edges_ubo = self.edges_ubo.load();

        let mut edge_width = edges_ubo.edge_width;

        let mut edge_color = [
            edges_ubo.edge_color.r,
            edges_ubo.edge_color.g,
            edges_ubo.edge_color.b,
        ];

        let edge_width_slider = ui.add(
            egui::Slider::new::<f32>(&mut edge_width, 0.5..=5.0)
                .text("Edge width"),
        );

        if edge_width_slider.changed() {
            edges_ubo.edge_width = edge_width;

            self.edges_ubo.store(edges_ubo);
        }

        let edge_color_picker = ui.color_edit_button_rgb(&mut edge_color);

        if edge_color_picker.changed() {
            let new_color =
                rgb::RGB::new(edge_color[0], edge_color[1], edge_color[2]);

            edges_ubo.edge_color = new_color;

            self.edges_ubo.store(edges_ubo);
        }

        if edges_button.clicked() {
            self.edges_enabled.store(!edges_enabled);
        }

        if min_node_width_slider.changed() {
            self.node_width.set_min_node_width(min_width);
        }

        if max_node_width_slider.changed() {
            self.node_width.set_max_node_width(max_width);
        }

        if min_scale_slider.changed() {
            self.node_width.set_min_node_scale(min_scale);
        }

        if max_scale_slider.changed() {
            self.node_width.set_max_node_scale(max_scale);
        }

        let mut label_radius = self.label_radius.load();

        let label_radius_slider = ui.add(
            egui::Slider::new::<f32>(&mut label_radius, 10.0..=200.0)
                .text("Label stacking radius"),
        );

        if label_radius_slider.changed() {
            self.label_radius.store(label_radius);
        }
    }
}
