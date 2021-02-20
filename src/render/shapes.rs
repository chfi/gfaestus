use vulkano::buffer::{BufferUsage, CpuAccessibleBuffer};
use vulkano::command_buffer::{
    AutoCommandBuffer, AutoCommandBufferBuilder, DynamicState,
};
use vulkano::device::Queue;
use vulkano::framebuffer::{RenderPassAbstract, Subpass};

use vulkano::pipeline::{GraphicsPipeline, GraphicsPipelineAbstract};

use std::sync::Arc;

use anyhow::Result;

use rgb::*;

use crate::geometry::*;

#[derive(Default, Debug, Clone, Copy)]
pub struct ShapeVertex {
    pub position: [f32; 2],
}

vulkano::impl_vertex!(ShapeVertex, position);

mod vs {
    vulkano_shaders::shader! {
        ty: "vertex",
        path: "shaders/shapes/vertex.vert",
    }
}

mod fs {
    vulkano_shaders::shader! {
        ty: "fragment",
        path: "shaders/shapes/fragment.frag",
    }
}

const DRAW_CIRCLE: u32 = 1;
const DRAW_RECT: u32 = 2;
const DRAW_INVERTED: u32 = 4;
// const DRAW_INSIDE: u32 = 8;
// const DRAW_OUTSIDE: u32 = 16;

fn rect_push_constant(
    color: RGBA<f32>,
    viewport_dims: [f32; 2],
    p0: Point,
    p1: Point,
    invert: bool,
) -> fs::ty::PushConstantData {
    let mut draw_flags = DRAW_RECT;
    if invert {
        draw_flags |= DRAW_INVERTED;
    }

    let top = p0.y.min(p1.y);
    let left = p0.x.min(p1.x);

    let bottom = p0.y.max(p1.y);
    let right = p0.x.max(p1.x);

    fs::ty::PushConstantData {
        color: [color.r, color.g, color.b, color.a],
        draw_flags,
        rect: [top, left, bottom, right],
        circle: [0.0, 0.0],
        radius: 0.0,
        border: 0.0025,
        screen_dims: viewport_dims,
        _dummy0: [0; 12],
        _dummy1: [0; 4],
        _dummy2: [0; 4],
    }
}

fn circle_push_constant(
    color: RGBA<f32>,
    viewport_dims: [f32; 2],
    center: Point,
    radius: f32,
    invert: bool,
) -> fs::ty::PushConstantData {
    let mut draw_flags = DRAW_CIRCLE;
    if invert {
        draw_flags |= DRAW_INVERTED;
    }

    fs::ty::PushConstantData {
        color: [color.r, color.g, color.b, color.a],
        draw_flags,
        rect: [0.0; 4],
        circle: [center.x, center.y],
        radius,
        border: 0.0025,
        screen_dims: viewport_dims,
        _dummy0: [0; 12],
        _dummy1: [0; 4],
        _dummy2: [0; 4],
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Primitive {
    // Line {
    //     p0: Point,
    //     p1: Point,
    // },
    Rect {
        top_left: Point,
        bottom_right: Point,
    },
    Circle {
        center: Point,
        radius: f32,
    },
    // Polygon {
    //     start: Point,
    //     rest: Vec<Point>,
    // },
}

impl Primitive {
    // pub fn line(p0: Point, p1: Point) -> Self {
    //     Self::Line { p0, p1 }
    // }

    pub fn rect(top_left: Point, bottom_right: Point) -> Self {
        Self::Rect {
            top_left,
            bottom_right,
        }
    }

    pub fn circle(center: Point, radius: f32) -> Self {
        Self::Circle { center, radius }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum DrawStyle {
    Filled {
        color: RGBA<f32>,
    },
    Border {
        color: RGBA<f32>,
        width: f32,
    },
    BorderFilled {
        border_color: RGBA<f32>,
        border_width: f32,
        fill_color: RGBA<f32>,
    },
}

impl DrawStyle {
    pub fn filled(color: RGBA<f32>) -> Self {
        Self::Filled { color }
    }

    pub fn border(color: RGBA<f32>, width: f32) -> Self {
        Self::Border { color, width }
    }

    pub fn border_filled(
        fill_color: RGBA<f32>,
        border_color: RGBA<f32>,
        border_width: f32,
    ) -> Self {
        Self::BorderFilled {
            border_color,
            border_width,
            fill_color,
        }
    }

    pub fn color(&self) -> RGBA<f32> {
        match self {
            DrawStyle::Filled { color } => *color,
            DrawStyle::Border { color, .. } => *color,
            DrawStyle::BorderFilled { border_color, .. } => *border_color,
            // DrawStyle::BorderFilled { border_color, border_width, fill_color } => { border_color }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    offset: Point,
    primitive: Primitive,
    style: DrawStyle,
}

impl Shape {
    pub fn from_primitive(primitive: Primitive) -> Self {
        let style = DrawStyle::filled(RGBA::new(1.0, 1.0, 1.0, 1.0));
        let offset = Point { x: 0.0, y: 0.0 };
        Self {
            primitive,
            style,
            offset,
        }
    }

    pub fn shift(mut self, offset: Point) -> Self {
        self.offset += offset;
        self
    }

    pub fn border(mut self, color: RGBA<f32>, width: f32) -> Self {
        self.style = DrawStyle::border(color, width);
        self
    }

    pub fn filled(mut self, color: RGBA<f32>) -> Self {
        self.style = DrawStyle::filled(color);
        self
    }

    pub fn border_filled(
        mut self,
        fill: RGBA<f32>,
        border: RGBA<f32>,
        width: f32,
    ) -> Self {
        self.style = DrawStyle::border_filled(fill, border, width);
        self
    }

    pub fn set_primitive(&mut self, primitive: Primitive) {
        self.primitive = primitive;
    }

    pub fn style_mut(&mut self) -> &mut DrawStyle {
        &mut self.style
    }

    pub fn rect(top_left: Point, bottom_right: Point) -> Self {
        let primitive = Primitive::rect(top_left, bottom_right);
        let style = DrawStyle::filled(RGBA::new(1.0, 1.0, 1.0, 1.0));
        let offset = Point { x: 0.0, y: 0.0 };
        Self {
            primitive,
            style,
            offset,
        }
    }
}

pub struct ShapeDrawSystem {
    gfx_queue: Arc<Queue>,
    vertex_buffer: Arc<CpuAccessibleBuffer<[ShapeVertex]>>,
    // vertex_buffer_pool: CpuBufferPool<ShapeVertex>,
    // color_buffer_pool: CpuBufferPool<Color>,
    pipeline: Arc<dyn GraphicsPipelineAbstract + Send + Sync>,
}

impl ShapeDrawSystem {
    pub fn new<R>(gfx_queue: Arc<Queue>, subpass: Subpass<R>) -> ShapeDrawSystem
    where
        R: RenderPassAbstract + Send + Sync + 'static,
    {
        let _ = include_str!("../../shaders/shapes/fragment.frag");
        let _ = include_str!("../../shaders/shapes/vertex.vert");

        let vs = vs::Shader::load(gfx_queue.device().clone()).unwrap();
        let fs = fs::Shader::load(gfx_queue.device().clone()).unwrap();

        let vertex_buffer = {
            CpuAccessibleBuffer::from_iter(
                gfx_queue.device().clone(),
                BufferUsage::all(),
                false,
                [
                    ShapeVertex {
                        position: [-1.0, -1.0],
                    },
                    ShapeVertex {
                        position: [-1.0, 3.0],
                    },
                    ShapeVertex {
                        position: [3.0, -1.0],
                    },
                ]
                .iter()
                .cloned(),
            )
            .expect("ShapeDrawSystem failed to create vertex buffer")
        };

        let pipeline = {
            Arc::new(
                GraphicsPipeline::start()
                    .vertex_input_single_buffer::<ShapeVertex>()
                    .vertex_shader(vs.main_entry_point(), ())
                    .triangle_list()
                    .viewports_dynamic_scissors_irrelevant(1)
                    .fragment_shader(fs.main_entry_point(), ())
                    .render_pass(subpass)
                    .blend_alpha_blending()
                    .build(gfx_queue.device().clone())
                    .unwrap(),
            ) as Arc<_>
        };

        ShapeDrawSystem {
            gfx_queue,
            pipeline,
            vertex_buffer,
        }
    }

    pub fn draw_shape(
        &self,
        dynamic_state: &DynamicState,
        shape: Shape,
    ) -> Result<AutoCommandBuffer> {
        let mut builder: AutoCommandBufferBuilder =
            AutoCommandBufferBuilder::secondary_graphics(
                self.gfx_queue.device().clone(),
                self.gfx_queue.family(),
                self.pipeline.clone().subpass(),
            )?;

        let viewport_dims = {
            let viewport = dynamic_state
                .viewports
                .as_ref()
                .and_then(|v| v.get(0))
                .unwrap();
            viewport.dimensions
        };

        let color = shape.style.color();

        let push_constants = match shape.primitive {
            Primitive::Rect {
                top_left,
                bottom_right,
            } => rect_push_constant(
                color,
                viewport_dims,
                top_left,
                bottom_right,
                false,
            ),
            Primitive::Circle { center, radius } => circle_push_constant(
                color,
                viewport_dims,
                center,
                radius,
                false,
            ),
        };

        builder.draw(
            self.pipeline.clone(),
            dynamic_state,
            vec![self.vertex_buffer.clone()],
            (),
            push_constants,
        )?;

        let builder = builder.build()?;

        Ok(builder)
    }

    pub fn draw_circle(
        &self,
        dynamic_state: &DynamicState,
        circle_at: Point,
        circle_rad: f32,
        invert: bool,
    ) -> Result<AutoCommandBuffer> {
        let mut builder: AutoCommandBufferBuilder =
            AutoCommandBufferBuilder::secondary_graphics(
                self.gfx_queue.device().clone(),
                self.gfx_queue.family(),
                self.pipeline.clone().subpass(),
            )?;

        let viewport_dims = {
            let viewport = dynamic_state
                .viewports
                .as_ref()
                .and_then(|v| v.get(0))
                .unwrap();
            viewport.dimensions
        };

        let push_constants = circle_push_constant(
            RGBA::new(1.0, 1.0, 1.0, 1.0),
            viewport_dims,
            circle_at,
            circle_rad,
            invert,
        );

        builder.draw(
            self.pipeline.clone(),
            dynamic_state,
            vec![self.vertex_buffer.clone()],
            (),
            push_constants,
        )?;

        let builder = builder.build()?;

        Ok(builder)
    }

    pub fn draw_rect(
        &self,
        dynamic_state: &DynamicState,
        p0: Point,
        p1: Point,
        invert: bool,
    ) -> Result<AutoCommandBuffer> {
        let mut builder: AutoCommandBufferBuilder =
            AutoCommandBufferBuilder::secondary_graphics(
                self.gfx_queue.device().clone(),
                self.gfx_queue.family(),
                self.pipeline.clone().subpass(),
            )?;

        let viewport_dims = {
            let viewport = dynamic_state
                .viewports
                .as_ref()
                .and_then(|v| v.get(0))
                .unwrap();
            viewport.dimensions
        };

        let push_constants = rect_push_constant(
            RGBA::new(1.0, 1.0, 1.0, 1.0),
            viewport_dims,
            p0,
            p1,
            invert,
        );

        builder.draw(
            self.pipeline.clone(),
            dynamic_state,
            vec![self.vertex_buffer.clone()],
            (),
            push_constants,
        )?;

        let builder = builder.build()?;

        Ok(builder)
    }
}
