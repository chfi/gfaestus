use handlegraph::handle::{Handle, NodeId};

use nalgebra_glm as glm;

use crate::{
    geometry::{Point, Rect},
    universe::Node,
    view::View,
};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LabelPos {
    World {
        point: Point,
        offset: Option<Point>,
    },
    Handle {
        handle: Handle,
        offset: Option<Point>,
    },
}

impl LabelPos {
    pub fn offset(&self, nodes: &[Node]) -> Option<Point> {
        match *self {
            LabelPos::World { offset, .. } => offset,
            LabelPos::Handle { handle, offset } => {
                // if the offset field isn't set, we use the handle
                // orientation and node position to figure it out

                if let Some(offset) = offset {
                    return Some(offset);
                }

                let id = handle.id();
                let ix = id.0 - 1;
                let node = nodes[ix as usize];

                let start_p = node.p0;
                let end_p = node.p1;

                let start_v = glm::vec2(start_p.x, start_p.y);
                let end_v = glm::vec2(end_p.x, end_p.y);

                let del = end_v - start_v;
                let rot_del =
                    glm::rotate_vec2(&del, std::f32::consts::PI / 2.0);

                let rot_del_norm = rot_del.normalize();

                let offset = Point::new(rot_del_norm[0], rot_del_norm[1]);

                Some(offset)
            }
        }
    }

    pub fn try_world(&self) -> Option<Point> {
        if let LabelPos::World { point, .. } = *self {
            Some(point)
        } else {
            None
        }
    }

    pub fn world(&self, nodes: &[Node]) -> Point {
        match *self {
            LabelPos::World { point, .. } => point,
            LabelPos::Handle { handle, .. } => {
                let id = handle.id();
                let ix = id.0 - 1;
                let node = nodes[ix as usize];
                node.center()
            }
        }
    }

    pub fn anchor(&self, nodes: &[Node]) -> egui::Align2 {
        let dir = if let Some(offset) = self.offset(nodes) {
            offset
        } else {
            return egui::Align2::CENTER_CENTER;
        };

        let norm = dir / dir.length();

        let align_for = |v: f32| {
            if v > 0.67 {
                egui::Align::Max
            } else if v < -0.67 {
                egui::Align::Min
            } else {
                egui::Align::Center
            }
        };

        let hor_align = align_for(norm.x);
        let ver_align = align_for(norm.y);

        egui::Align2([hor_align, ver_align])
    }
}

pub fn offset_align(dir: &Point) -> egui::Align2 {
    let norm = *dir / dir.length();

    let align_for = |v: f32| {
        if v > 0.67 {
            egui::Align::Max
        } else if v < -0.67 {
            egui::Align::Min
        } else {
            egui::Align::Center
        }
    };

    let hor_align = align_for(norm.x);
    let ver_align = align_for(norm.y);

    egui::Align2([hor_align, ver_align])
}

pub fn draw_text_at_node_anchor(
    ctx: &egui::CtxRef,
    node_positions: &[Node],
    view: View,
    node: NodeId,
    screen_offset: Point,
    anchor_dir: Point,
    text: &str,
) -> Option<Rect> {
    let node_ix = (node.0 - 1) as usize;

    if let Some(node) = node_positions.get(node_ix) {
        let pos = node.center();

        return draw_text_at_aligned_world_point_offset(
            ctx,
            view,
            pos,
            screen_offset,
            anchor_dir,
            text,
        );
    }

    None
}

pub fn draw_text_at_world_point(
    ctx: &egui::CtxRef,
    view: View,
    world: Point,
    text: &str,
) -> Option<Rect> {
    draw_text_at_world_point_offset(ctx, view, world, Point::ZERO, text)
}

pub fn draw_text_at_node(
    ctx: &egui::CtxRef,
    node_positions: &[Node],
    view: View,
    node: NodeId,
    screen_offset: Point,
    text: &str,
) -> Option<Rect> {
    let node_ix = (node.0 - 1) as usize;

    if let Some(node) = node_positions.get(node_ix) {
        let pos = node.center();

        return draw_text_at_world_point_offset(
            ctx,
            view,
            pos,
            screen_offset,
            text,
        );
    }

    None
}

pub fn draw_text_at_world_point_offset(
    ctx: &egui::CtxRef,
    view: View,
    world: Point,
    screen_offset: Point,
    text: &str,
) -> Option<Rect> {
    draw_text_at_aligned_world_point_offset(
        ctx,
        view,
        world,
        screen_offset,
        Point::ZERO,
        text,
    )
}

fn painter_layer() -> egui::LayerId {
    egui::LayerId::new(
        egui::Order::Background,
        egui::Id::new("gui_text_background"),
    )
}

pub fn draw_point_world(ctx: &egui::CtxRef, view: View, point: Point) {
    let screen_rect = ctx.input().screen_rect();
    let offset = Point::new(screen_rect.width(), screen_rect.height()) / 2.0;

    let mut s = view.world_point_to_screen(point);
    s += offset;

    let painter = ctx.layer_painter(painter_layer());
    let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(128, 128, 128));

    painter.circle_stroke(point.into(), 2.0, stroke);
}

pub fn draw_circle_world(
    ctx: &egui::CtxRef,
    view: View,
    origin: Point,
    radius: f32,
    color: Option<rgb::RGBA<f32>>,
) {
    let screen_rect = ctx.input().screen_rect();

    // let screen_radius = radius * view.scale;
    let screen_radius = radius;

    let p = origin;

    let mut s = view.world_point_to_screen(p);

    let offset = Point::new(screen_rect.width(), screen_rect.height()) / 2.0;

    s += offset;

    let painter = ctx.layer_painter(painter_layer());

    let color = color
        .map(|c| {
            let r = (c.r * 255.0) as u8;
            let g = (c.g * 255.0) as u8;
            let b = (c.b * 255.0) as u8;
            let a = (c.a * 255.0) as u8;
            egui::Color32::from_rgba_unmultiplied(r, g, b, a)
        })
        .unwrap_or(egui::Color32::from_rgb(128, 128, 128));

    let stroke = egui::Stroke::new(2.0, color);

    painter.circle_stroke(s.into(), screen_radius, stroke);
    // painter.rect_stroke(Rect::new(s0, s1).into(), 0.0, stroke);
}

pub fn draw_rect_world(
    ctx: &egui::CtxRef,
    view: View,
    rect: Rect,
    color: Option<rgb::RGBA<f32>>,
) {
    let screen_rect = ctx.input().screen_rect();

    let p0 = rect.min();
    let p1 = rect.max();

    let mut s0 = view.world_point_to_screen(p0);
    let mut s1 = view.world_point_to_screen(p1);

    let offset = Point::new(screen_rect.width(), screen_rect.height()) / 2.0;

    s0 += offset;
    s1 += offset;

    let painter = ctx.layer_painter(painter_layer());

    let color = color
        .map(|c| {
            let r = (c.r * 255.0) as u8;
            let g = (c.g * 255.0) as u8;
            let b = (c.b * 255.0) as u8;
            let a = (c.a * 255.0) as u8;
            egui::Color32::from_rgba_unmultiplied(r, g, b, a)
        })
        .unwrap_or(egui::Color32::from_rgb(128, 128, 128));

    let stroke = egui::Stroke::new(2.0, color);

    painter.rect_stroke(Rect::new(s0, s1).into(), 0.0, stroke);
}

pub fn draw_rect<R: Into<egui::Rect>>(ctx: &egui::CtxRef, rect: R) {
    let painter = ctx.layer_painter(painter_layer());

    let stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(128, 128, 128));

    let rect = rect.into();

    painter.rect_stroke(rect, 0.0, stroke);
}

pub fn draw_text_at_aligned_world_point_offset(
    ctx: &egui::CtxRef,
    view: View,
    world: Point,
    screen_offset: Point,
    anchor_dir: Point,
    text: &str,
) -> Option<Rect> {
    let screen_rect = ctx.input().screen_rect();

    let painter = ctx.layer_painter(painter_layer());

    let screen_pos = view.world_point_to_screen(world);

    let dims = Point::new(screen_rect.width(), screen_rect.height());

    let mut screen_pos = screen_pos + dims / 2.0;
    screen_pos += screen_offset;

    // hacky way to ensure that the text is only being rendered when
    // (more or less) on the screen, without being cut off if the
    // center of the text is just outside the visible area
    if screen_pos.x > -screen_rect.width()
        && screen_pos.x < 2.0 * screen_rect.width()
        && screen_pos.y > -screen_rect.height()
        && screen_pos.y < 2.0 * screen_rect.height()
    {
        let align = offset_align(&anchor_dir);

        let rect = painter.text(
            screen_pos.into(),
            align,
            text,
            // egui::TextStyle::Body,
            egui::TextStyle::Button,
            ctx.style().visuals.text_color(),
        );

        return Some(rect.into());
    }

    None
}
