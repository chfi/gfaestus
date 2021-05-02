use gluon::*;

use anyhow::Result;
use vm::api::FunctionRef;

use crate::vulkan::draw_system::nodes::overlay::NodeOverlay;

pub struct GluonVM {
    vm: RootedThread,
}

pub type RGBTuple = (f32, f32, f32, f32);

impl GluonVM {
    pub fn new() -> Self {
        let vm = new_vm();
        Self { vm }
    }

    pub fn run_overlay_expr(&self, expr_str: &str) -> Result<Vec<RGBTuple>> {
        self.vm.run_io(true);
        let (res, _arc) = self.vm.run_expr("overlay_expr", expr_str)?;
        self.vm.run_io(false);
        match res {
            vm::api::IO::Value(v) => Ok(v),
            vm::api::IO::Exception(err) => {
                anyhow::bail!(err)
            }
        }
    }

    pub fn example_overlay(&self, node_count: usize) -> gluon::Result<Vec<rgb::RGB<f32>>> {
        let script = r#"
let int = import! std.int

let color_fn node_id =
    if int.rem node_id 2 == 0 then (0.5, 0.5, 0.5) else (0.8, 0.3, 0.3)
color_fn
"#;

        let (mut node_color, _): (FunctionRef<fn(u32) -> (f32, f32, f32)>, _) =
            self.vm.run_expr("node_color_fun", script)?;

        let mut colors: Vec<rgb::RGB<f32>> = Vec::with_capacity(node_count);

        for node_id in 0..node_count {
            let node_id = node_id as u32;
            let (r, g, b) = node_color.call(node_id)?;

            colors.push(rgb::RGB::new(r, g, b));
        }

        Ok(colors)
    }
}
