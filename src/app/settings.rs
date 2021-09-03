use crossbeam::atomic::AtomicCell;
use std::sync::Arc;

use crate::vulkan::draw_system::edges::EdgesUBO;

#[derive(Debug, Clone)]
pub struct AppSettings {
    node_width: Arc<NodeWidth>,

    edge_renderer: Arc<AtomicCell<EdgesUBO>>,

    label_radius: Arc<AtomicCell<f32>>,

    background_color_light: Arc<AtomicCell<rgb::RGB<f32>>>,
    background_color_dark: Arc<AtomicCell<rgb::RGB<f32>>>,
}

impl std::default::Default for AppSettings {
    fn default() -> Self {
        Self {
            node_width: Default::default(),
            edge_renderer: Default::default(),
            label_radius: Arc::new(50.0.into()),

            background_color_light: Arc::new(
                rgb::RGB::new(1.0, 1.0, 1.0).into(),
            ),
            background_color_dark: Arc::new(
                rgb::RGB::new(0.1, 0.1, 0.2).into(),
            ),
        }
    }
}

impl AppSettings {
    pub fn node_width(&self) -> &Arc<NodeWidth> {
        &self.node_width
    }

    pub fn edge_renderer(&self) -> &Arc<AtomicCell<EdgesUBO>> {
        &self.edge_renderer
    }

    pub fn update_edge_renderer(&self, conf: EdgesUBO) {
        self.edge_renderer.store(conf);
    }

    pub fn label_radius(&self) -> &Arc<AtomicCell<f32>> {
        &self.label_radius
    }

    pub fn background_color_light(&self) -> &Arc<AtomicCell<rgb::RGB<f32>>> {
        &self.background_color_light
    }

    pub fn background_color_dark(&self) -> &Arc<AtomicCell<rgb::RGB<f32>>> {
        &self.background_color_dark
    }
}

#[derive(Debug)]
pub struct NodeWidth {
    min_node_width: AtomicCell<f32>,
    max_node_width: AtomicCell<f32>,

    min_node_scale: AtomicCell<f32>,
    max_node_scale: AtomicCell<f32>,
}

impl NodeWidth {
    pub fn min_node_width(&self) -> f32 {
        self.min_node_width.load()
    }

    pub fn max_node_width(&self) -> f32 {
        self.max_node_width.load()
    }
    pub fn min_node_scale(&self) -> f32 {
        self.min_node_scale.load()
    }

    pub fn max_node_scale(&self) -> f32 {
        self.max_node_scale.load()
    }

    pub fn set_min_node_width(&self, width: f32) {
        self.min_node_width.store(width);
    }

    pub fn set_max_node_width(&self, width: f32) {
        self.max_node_width.store(width);
    }

    pub fn set_min_node_scale(&self, width: f32) {
        self.min_node_scale.store(width);
    }

    pub fn set_max_node_scale(&self, width: f32) {
        self.max_node_scale.store(width);
    }
}

impl std::default::Default for NodeWidth {
    fn default() -> Self {
        Self {
            min_node_width: AtomicCell::new(5.0),
            max_node_width: AtomicCell::new(150.0),

            min_node_scale: AtomicCell::new(1.0),
            max_node_scale: AtomicCell::new(50.0),
        }
    }
}
