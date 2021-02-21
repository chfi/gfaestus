use crate::geometry::*;

#[allow(unused_imports)]
use super::{grid, Universe};

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

#[derive(Debug, Clone, Copy)]
pub struct UniverseBuilder {
    pub bp_per_world_unit: f32,
    pub physics_config: PhysicsConfig,
    pub layout_config: LayoutConfig,
    pub cell_size: f32,
    pub grid_columns: usize,
    pub grid_rows: usize,
    pub view_config: ViewConfig,
}

/*
impl UniverseBuilder {
    pub fn build(self) -> Universe {
        let grid_dims = grid::GridDims {
            columns: self.grid_columns,
            rows: self.grid_rows,
        };

        let cell_dims = grid::CellDims {
            width: self.cell_size,
            height: self.cell_size,
        };

        let offset = Point { x: 0.0, y: 0.0 };

        let grid = grid::Grid::new(offset, grid_dims, cell_dims);

        Universe {
            bp_per_world_unit: self.bp_per_world_unit,
            grid,
            offset,
            angle: 0.0,
            physics_config: self.physics_config,
            layout_config: self.layout_config,
            view_config: self.view_config,
        }
    }
}
*/

#[derive(Debug, Clone, Copy)]
pub struct LayoutConfig {
    pub node_width: f32,
    pub neighbor_node_pad: f32,
    pub parallel_node_pad: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct ViewConfig {
    pub init_view_offset: Point,
    pub init_view_scale: f32,
    pub min_view_scale: f32,
    pub max_view_scale: Option<f32>,
}

#[derive(Debug, Clone, Copy)]
pub struct PhysicsConfig {
    pub enable_physics: bool,
    pub tick_len: std::time::Duration,
    pub tick_rate_hz: f32,
    pub max_interact_dist: f32,
    pub charge_per_bp: f32,
    pub min_node_charge: f32,
    pub charge_dist_mult: f32,
    pub repulsion_mult: f32,
    pub attraction_mult: f32,
    // pub mass_per_bp: f32,
    // pub min_node_mass: f32,
    // pub gravity_dist_mult: f32,
    // pub gravity_mult: f32,
    // pub edge_min_len: f32,
    // pub edge_max_len: f32,
    // pub edge_force_mult: f32,
    // pub edge_stiffness: f32,
    // pub edge_angle_stiffness: f32,
}

impl std::default::Default for PhysicsConfig {
    fn default() -> Self {
        Self {
            enable_physics: true,
            tick_len: std::time::Duration::from_millis(20),
            tick_rate_hz: 60.0,
            max_interact_dist: 10000.0,
            charge_per_bp: 1.0,
            min_node_charge: 1.0,
            charge_dist_mult: 1.0,
            repulsion_mult: 1.0,
            attraction_mult: 1.0,
        }
    }
}

impl std::default::Default for LayoutConfig {
    fn default() -> Self {
        Self {
            node_width: 20.0,
            neighbor_node_pad: 20.0,
            parallel_node_pad: 20.0,
        }
    }
}

impl std::default::Default for ViewConfig {
    fn default() -> Self {
        ViewConfig {
            init_view_offset: Point { x: 0.0, y: 0.0 },
            init_view_scale: 1.0,
            min_view_scale: 1.0,
            max_view_scale: None,
        }
    }
}

impl std::default::Default for UniverseBuilder {
    fn default() -> Self {
        Self {
            bp_per_world_unit: 1.0,
            physics_config: PhysicsConfig::default(),
            layout_config: LayoutConfig::default(),
            cell_size: 1024.0,
            grid_columns: 10,
            grid_rows: 10,
            view_config: ViewConfig::default(),
        }
    }
}
