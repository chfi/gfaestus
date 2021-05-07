use crossbeam::atomic::AtomicCell;
use std::sync::Arc;

use crate::app::NodeWidth;

pub struct MainViewSettings {
    node_width: Arc<NodeWidth>,
}

impl MainViewSettings {
    const ID: &'static str = "main_view_settings_window";

    pub fn new(node_width: Arc<NodeWidth>) -> Self {
        Self { node_width }
    }

    pub fn ui(&mut self, ctx: &egui::CtxRef) -> Option<egui::Response> {
        egui::Window::new("View Settings")
            .id(egui::Id::new(Self::ID))
            .show(ctx, |ui| {
                let mut base_node_width = self.node_width.base_node_width();
                let mut upscale_limit = self.node_width.upscale_limit();
                let mut upscale_factor = self.node_width.upscale_factor();

                let node_width_slider = ui.add(
                    egui::Slider::new::<f32>(&mut base_node_width, 10.0..=300.0).text("Node width"),
                );

                let upscale_limit_slider = ui.add(
                    egui::Slider::new::<f32>(&mut upscale_limit, 10.0..=300.0)
                        .text("Upscale limit"),
                );

                let upscale_factor_slider = ui.add(
                    egui::Slider::new::<f32>(&mut upscale_factor, 10.0..=300.0)
                        .text("Upscale factor"),
                );

                if node_width_slider.changed() {
                    self.node_width.set_base_node_width(base_node_width);
                }

                if upscale_limit_slider.changed() {
                    self.node_width.set_upscale_limit(upscale_limit);
                }

                if upscale_factor_slider.changed() {
                    self.node_width.set_upscale_factor(upscale_factor);
                }
            })
    }
}
