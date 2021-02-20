pub mod gui;
pub mod lines;
pub mod nodes;
pub mod shapes;

pub use gui::GuiDrawSystem;
pub use lines::LineDrawSystem;
pub use nodes::NodeDrawSystem;
pub use shapes::ShapeDrawSystem;

pub type PoolChunk<T> = vulkano::buffer::cpu_pool::CpuBufferPoolChunk<
    T,
    std::sync::Arc<vulkano::memory::pool::StdMemoryPool>,
>;
pub type SubPoolChunk<T> = vulkano::buffer::cpu_pool::CpuBufferPoolSubbuffer<
    T,
    std::sync::Arc<vulkano::memory::pool::StdMemoryPool>,
>;

#[derive(Default, Debug, Clone, Copy)]
pub struct Vertex {
    pub position: [f32; 2],
}

vulkano::impl_vertex!(Vertex, position);

#[derive(Default, Debug, Clone, Copy)]
pub struct Color {
    pub color: [f32; 3],
}

vulkano::impl_vertex!(Color, color);
