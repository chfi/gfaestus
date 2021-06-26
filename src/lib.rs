pub mod app;
pub mod geometry;
pub mod vulkan;

pub mod annotations;
pub mod graph_query;
pub mod gui;
pub mod overlays;

pub mod gfa;
pub mod universe;

pub mod input;
pub mod view;

pub mod asynchronous;
pub mod gluon;

#[macro_export]
macro_rules! include_shader {
    ($file:expr) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/", $file))
    };
}
