use crossbeam::atomic::AtomicCell;
use std::sync::Arc;

use crate::app::NodeWidth;

pub struct MainViewSettings {
    node_width: Arc<NodeWidth>,
    edges_enabled: Arc<AtomicCell<bool>>,
}

impl MainViewSettings {
    const ID: &'static str = "main_view_settings_window";

    pub fn new(
        node_width: Arc<NodeWidth>,
        edges_enabled: Arc<AtomicCell<bool>>,
    ) -> Self {
        Self {
            node_width,
            edges_enabled,
        }
    }

    pub fn ui(&mut self, ctx: &egui::CtxRef) -> Option<egui::Response> {
        egui::Window::new("View Settings")
            .id(egui::Id::new(Self::ID))
            .show(ctx, |ui| {


                let mut min_width = self.node_width.min_node_width();
                let mut max_width = self.node_width.max_node_width();

                let mut min_scale = self.node_width.min_scale();
                let mut max_scale = self.node_width.max_scale();

                let edges_enabled = self.edges_enabled.load();

                // let mut base_node_width = self.node_width.base_node_width();
                // let mut upscale_limit = self.node_width.upscale_limit();
                // let mut upscale_factor = self.node_width.upscale_factor();

                let min_node_width_slider = ui.add(
                    egui::Slider::new::<f32>(&mut min_width, 0.1..=max_width).text("Min node width"),
                ).on_hover_text("The minimum node width, in pixels at scale 1.0. Default: 0.1");

                let max_node_width_slider = ui.add(
                    egui::Slider::new::<f32>(&mut max_width, min_width..=300.0).text("Max node width"),
                ).on_hover_text("The maximum node width, in pixels at scale 1.0. Default: 100.0");


                let min_scale_slider = ui.add(
                    egui::Slider::new::<f32>(&mut min_scale, 1.0..=max_scale).text("Min node width scale"),
                ).on_hover_text("The scale below which the minimum node width will be used. Default: 0.1");

                let max_scale_slider = ui.add(
                    egui::Slider::new::<f32>(&mut max_scale, min_scale..=1000.0).text("Max node width scale"),
                ).on_hover_text("The scale above which the maximum node width will be used. Default: 200.0");

                let edges_button = ui.selectable_label(edges_enabled, "Show Edges");

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
                    self.node_width.set_min_scale(min_scale);
                }

                if max_scale_slider.changed() {
                    self.node_width.set_max_scale(max_scale);
                }
            })
    }
}
