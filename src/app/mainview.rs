use crossbeam::channel;
use std::sync::Arc;
use std::time::Instant;
use vulkano::sync::{self, FlushError, GpuFuture};

use rgb::*;

use nalgebra_glm as glm;

use anyhow::{Context, Result};

use crate::geometry::*;
use crate::gfa::*;
use crate::input::*;
use crate::layout::physics;
use crate::layout::*;
use crate::render::*;
use crate::ui::{UICmd, UIState, UIThread};
use crate::view;
use crate::view::View;

pub struct MainView {
    node_draw_system: NodeDrawSystem,
    view: View,
    vertices: Vec<Vertex>,
    draw_grid: bool,
    ui_thread: UIThread,
    ui_cmd_tx: channel::Sender<UICmd>,
}
