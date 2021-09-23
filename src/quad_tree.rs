use std::collections::VecDeque;

use crate::{geometry::*, universe::Node, view::*};

pub struct QuadTree<T: Clone> {
    boundary: Rect,

    points: Vec<Point>,
    data: Vec<T>,

    north_west: Option<Box<QuadTree<T>>>,
    north_east: Option<Box<QuadTree<T>>>,

    south_west: Option<Box<QuadTree<T>>>,
    south_east: Option<Box<QuadTree<T>>>,
}

impl<T: Clone> QuadTree<T> {
    pub const NODE_CAPACITY: usize = 4;

    pub fn new(boundary: Rect) -> Self {
        Self {
            boundary,

            points: Vec::new(),
            data: Vec::new(),

            north_west: None,
            north_east: None,

            south_west: None,
            south_east: None,
        }
    }

    pub fn node_len(&self) -> usize {
        self.points.len()
    }

    pub fn is_leaf(&self) -> bool {
        self.north_west.is_none()
    }

    fn insert_child(
        child: Option<&mut Self>,
        point: Point,
        data: T,
    ) -> std::result::Result<(), T> {
        if let Some(child) = child {
            return child.insert(point, data);
        }

        Err(data)
    }

    fn children(&self) -> Option<[&QuadTree<T>; 4]> {
        let nw = self.north_west.as_deref()?;
        let ne = self.north_east.as_deref()?;
        let sw = self.south_west.as_deref()?;
        let se = self.south_east.as_deref()?;

        let children = [nw, ne, sw, se];

        Some(children)
    }

    fn children_mut(&mut self) -> Option<[&mut QuadTree<T>; 4]> {
        let nw = self.north_west.as_deref_mut()?;
        let ne = self.north_east.as_deref_mut()?;
        let sw = self.south_west.as_deref_mut()?;
        let se = self.south_east.as_deref_mut()?;

        let children = [nw, ne, sw, se];

        Some(children)
    }

    pub fn insert(
        &mut self,
        point: Point,
        data: T,
    ) -> std::result::Result<(), T> {
        log::debug!(
            "inserting data into quad tree at ({}, {})",
            point.x,
            point.y
        );
        if !self.boundary.contains(point) {
            log::debug!("point is outside tree bounds, aborting");
            return Err(data);
        }

        if self.node_len() < Self::NODE_CAPACITY && self.is_leaf() {
            log::debug!("in leaf below capacity, adding to node");
            self.points.push(point);
            self.data.push(data);
            return Ok(());
        }

        if self.is_leaf() {
            log::debug!("subdividing node");
            self.subdivide();
        }

        let children = [
            self.north_west.as_deref_mut(),
            self.north_east.as_deref_mut(),
            self.south_west.as_deref_mut(),
            self.south_east.as_deref_mut(),
        ];

        let mut data = data;
        for child in children {
            match Self::insert_child(child, point, data) {
                Ok(_) => {
                    log::debug!("inserting point into child");
                    return Ok(());
                }
                Err(d) => {
                    log::debug!("skipping child");
                    data = d;
                }
            }
        }

        Err(data)
    }

    fn subdivide(&mut self) {
        let min = self.boundary.min();
        let max = self.boundary.max();

        let pt = |x, y| Point::new(x, y);

        let mid_x = min.x + ((max.x - min.x) / 2.0);
        let mid_y = min.y + ((max.y - min.y) / 2.0);

        // calculate the boundary rectangles for the children
        let top_left_bnd = Rect::new(pt(min.x, min.y), pt(mid_x, mid_y));
        let top_right_bnd = Rect::new(pt(mid_x, min.y), pt(max.x, mid_y));

        let btm_left_bnd = Rect::new(pt(min.x, mid_y), pt(mid_x, max.y));
        let btm_right_bnd = Rect::new(pt(mid_x, mid_y), pt(max.x, max.y));

        let mut top_left = Self::new(top_left_bnd);
        let mut top_right = Self::new(top_right_bnd);
        let mut btm_left = Self::new(btm_left_bnd);
        let mut btm_right = Self::new(btm_right_bnd);

        let move_to_child = |child: &mut Self| {
            let bnd = child.boundary;

            // TODO this should be done without cloning, but the data
            // will probably be Copy in most cases so it doesn't
            // matter for now
            for (point, data) in self.query_range(bnd) {
                if let Err(_) = child.insert(point, data.clone()) {
                    panic!("unexpected error when subdividing quadtree");
                }
            }
        };

        move_to_child(&mut top_left);
        move_to_child(&mut top_right);
        move_to_child(&mut btm_left);
        move_to_child(&mut btm_right);

        self.north_west = Some(Box::new(top_left));
        self.north_east = Some(Box::new(top_right));

        self.south_west = Some(Box::new(btm_left));
        self.south_east = Some(Box::new(btm_right));
    }

    pub fn query_range(&self, range: Rect) -> Vec<(Point, &T)> {
        let mut results = Vec::new();

        if !self.boundary.intersects(range) {
            return results;
        }

        for (&point, data) in self.points.iter().zip(self.data.iter()) {
            if range.contains(point) {
                results.push((point, data));
            }
        }

        if let Some(children) = self.children() {
            for child in children {
                results.extend(Self::child_range(child, range));
            }
        }

        results
    }

    fn child_range<'a>(
        child: &'a Self,
        range: Rect,
    ) -> impl Iterator<Item = (Point, &'a T)> {
        child.points.iter().zip(child.data.iter()).filter_map(
            move |(&point, data)| {
                if range.contains(point) {
                    Some((point, data))
                } else {
                    None
                }
            },
        )
    }

    pub fn rects(&self) -> Vec<Rect> {
        let mut result = Vec::new();

        let mut queue = VecDeque::new();

        queue.push_back(self);

        while let Some(node) = queue.pop_front() {
            result.push(node.boundary);

            if let Some(children) = node.children() {
                for child in children {
                    queue.push_back(child);
                }
            }
        }

        result
    }

    pub fn iter(&self) -> Iter<'_, T> {
        Iter::new(self)
    }
}

struct IterHelper<'a, T: Clone> {
    node: &'a QuadTree<T>,
    points: std::slice::Iter<'a, Point>,
    data: std::slice::Iter<'a, T>,
}

impl<'a, T: Clone> IterHelper<'a, T> {
    fn next(&mut self) -> Option<(Point, &'a T)> {
        let point = self.points.next()?;
        let data = self.data.next()?;

        Some((*point, data))
    }
}

pub struct Iter<'a, T: Clone> {
    queue: VecDeque<&'a QuadTree<T>>,

    current: Option<IterHelper<'a, T>>,

    finished: bool,
}

impl<'a, T: Clone> Iter<'a, T> {
    fn new(tree: &'a QuadTree<T>) -> Self {
        let mut queue = VecDeque::new();
        queue.push_back(tree);

        Self {
            queue,

            current: None,

            finished: false,
        }
    }

    fn next(&mut self) -> Option<(Point, &'a T)> {
        if self.finished {
            return None;
        }

        let result = self.current.as_mut().and_then(|cur| cur.next());
        if result.is_some() {
            return result;
        }

        let next = self.queue.pop_front();
        if next.is_none() {
            self.finished = false;
            return None;
        }

        let current = next?;

        if let Some(children) = current.children() {
            for child in children {
                self.queue.push_back(child);
            }
        }

        let mut current = IterHelper {
            node: current,
            points: current.points.iter(),
            data: current.data.iter(),
        };

        let result = current.next();
        self.current = Some(current);

        result
    }
}

impl<'a, T: Clone> Iterator for Iter<'a, T> {
    type Item = (Point, &'a T);

    fn next(&mut self) -> Option<Self::Item> {
        Iter::next(self)
    }
}

/*
pub struct QuadTree<const N: usize> {
    boundary: Rect,

    points: [Point; N],

    north_west: Option<Box<QuadTree<N>>>,
    north_east: Option<Box<QuadTree<N>>>,

    south_west: Option<Box<QuadTree<N>>>,
    south_east: Option<Box<QuadTree<N>>>,
}
*/
