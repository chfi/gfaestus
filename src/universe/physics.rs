// use crate::geometry::*;

/*
use super::Spine;

pub fn repulsion_spines(t: f32, spines: &mut [Spine]) {
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
                    let mag = t * (force_mult / dist_clamp.powi(2));

                    let this_force = toward * mag;
                    force += this_force;
                    if force.x.is_nan() || force.y.is_nan() {
                        println!(" node.p0: {}, {}", node.p0.x, node.p0.y);
                        println!(" node.p1: {}, {}", node.p1.x, node.p1.y);
                        println!("other.p0: {}, {}", other.p0.x, other.p0.y);
                        println!("other.p1: {}, {}", other.p1.x, other.p1.y);
                        std::process::exit(1);
                    }
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

*/
