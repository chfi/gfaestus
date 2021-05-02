use crossbeam::atomic::AtomicCell;
use std::sync::Arc;

pub struct MainViewSettings {
    node_width: Arc<AtomicCell<f32>>,
}

impl MainViewSettings {
    const ID: &'static str = "main_view_settings_window";

    pub fn new(node_width: Arc<AtomicCell<f32>>) -> Self {
        Self { node_width }
    }

    pub fn ui(&mut self, ctx: &egui::CtxRef) -> Option<egui::Response> {
        egui::Window::new("View Settings")
            .id(egui::Id::new(Self::ID))
            .show(ctx, |ui| {
                let mut node_width_local = self.node_width.load();

                ui.label("Node width");
                if ui
                    .add(egui::Slider::f32(&mut node_width_local, 10.0..=300.0))
                    .drag_released()
                {
                    self.node_width.store(node_width_local);
                }
            })
    }
}
