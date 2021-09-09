pub mod app;
pub mod geometry;
pub mod reactor;
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
// pub mod gluon;
pub mod script;

#[macro_export]
macro_rules! include_shader {
    ($file:expr) => {
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/shaders/", $file))
    };
}

#[macro_export]
macro_rules! load_shader {
    ($path:literal) => {{
        let buf = crate::include_shader!($path);
        let mut cursor = std::io::Cursor::new(buf);
        ash::util::read_spv(&mut cursor).unwrap()
    }};
}
