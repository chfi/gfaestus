#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    pathhandlegraph::*,
};

#[allow(unused_imports)]
use handlegraph::packedgraph::PackedGraph;

use nalgebra_glm as glm;

use anyhow::Result;

use crate::geometry::*;
use crate::render::{Color, Vertex};

pub mod config;
pub mod grid;
pub mod physics;

pub use config::*;

#[derive(Debug, Clone)]
pub struct Universe {
    bp_per_world_unit: f32,
    grid: grid::Grid<NodeId>,
    // node_ids: Vec<NodeId>,
    offset: Point,
    angle: f32,
    physics_config: PhysicsConfig,
    layout_config: LayoutConfig,
    view_config: ViewConfig,
}
