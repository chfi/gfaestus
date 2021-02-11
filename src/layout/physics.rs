use crate::geometry::*;
use crate::view::*;

use super::{Node, Spine};

// pub fn repulsion_spines(t: f32, spines: &mut Vec<Spine>) {
pub fn repulsion_spines(t: f32, spines: &mut [Spine]) {
    // let force_mult = 25.0;
    let force_mult = 1000.0;

    let mut forces: Vec<(usize, Vec<Point>)> = Vec::new();

    // let mut forces: Vec<(usize, Point)> = Vec::new();

    for (ix, spine) in spines.iter().enumerate() {
        let mut spine_forces: Vec<Point> = Vec::new();

        let s_offset = spine.offset;
        let _s_angle = spine.angle;

        for (n_ix, node) in spine.nodes.iter().enumerate() {
            let mut force = Point { x: 0.0, y: 0.0 };
            let n_mid = node.center() + s_offset;

            for (o_ix, o_spine) in spines.iter().enumerate() {
                let o_offset = o_spine.offset;
                let _o_angle = o_spine.angle;

                for (o_n_ix, other) in o_spine.nodes.iter().enumerate() {
                    if ix == o_ix && n_ix == o_n_ix {
                        continue;
                    }

                    let o_mid = other.center() + o_spine.offset;
                    let toward = n_mid.toward(o_mid);

                    let dist = n_mid.dist(o_mid);
                    let dist_clamp = 1.0_f32.max(dist);

                    // let mag = t * (force_mult / dist.powi(2));
                    // let mag = t * (force_mult / dist_clamp.powi(2));
                    let mag = (force_mult / dist_clamp.powi(2));

                    let this_force = toward * mag;
                    force += this_force;
                }
            }
            spine_forces.push(force);
        }
        forces.push((ix, spine_forces));
    }

    for (ix, spine) in spines.iter_mut().enumerate() {
        let (_, spine_forces) = &forces[ix];
        for (n_ix, node) in spine.nodes.iter_mut().enumerate() {
            let force = spine_forces[n_ix];
            node.p0 += force;
            node.p1 += force;
        }
    }
}

pub fn repulsion(t: f32, nodes: &mut [Node]) {
    let force_mult = 10000.0;

    let mut forces: Vec<Point> = Vec::new();

    for (ix, node) in nodes.iter().enumerate() {
        let mut force = Point { x: 0.0, y: 0.0 };

        let this_center = node.center();

        for (o_ix, other) in nodes.iter().enumerate() {
            if ix == o_ix {
                continue;
            }

            let other_center = other.center();

            let dist = this_center.dist(other_center);

            let toward = this_center.toward(other_center);
            // let away = toward * -1.0;

            let dist_clamp = 1.0_f32.max(dist);
            let mag = force_mult / dist_clamp.powi(2);
            // let mag = force_mult;

            let this_force = toward * mag;
            force += this_force;
        }

        forces.push(force)
    }

    // let f0 = forces[0];
    // let f1 = forces[1];
    // println!(
    //     "node 0 force: ({:.8}, {:.8})\tnode 1 force ({:.8}, {:.8})",
    //     f0.x, f0.y, f1.x, f1.y
    // );

    for (ix, node) in nodes.iter_mut().enumerate() {
        let force = forces[ix];

        node.p0 += force;
        node.p1 += force;
    }
}
