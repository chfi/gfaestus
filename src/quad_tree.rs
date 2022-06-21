use std::collections::VecDeque;

use crate::geometry::*;

#[derive(Debug, Clone)]
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
    pub const NODE_CAPACITY: usize = 128;

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

    pub fn boundary(&self) -> Rect {
        self.boundary
    }

    pub fn points(&self) -> &[Point] {
        &self.points
    }

    pub fn data(&self) -> &[T] {
        &self.data
    }

    pub fn elems<'a>(&'a self) -> impl Iterator<Item = (Point, &'a T)> {
        self.points.iter().copied().zip(self.data.iter())
    }

    pub fn node_len(&self) -> usize {
        self.points.len()
    }

    pub fn is_leaf(&self) -> bool {
        self.north_west.is_none()
    }

    fn can_insert(&self, point: Point) -> bool {
        if !self.boundary.contains(point) {
            return false;
        }

        true
    }

    pub fn insert(
        &mut self,
        point: Point,
        data: T,
    ) -> std::result::Result<(), T> {
        log::trace!(
            "inserting data into quad tree at ({}, {})",
            point.x,
            point.y
        );
        if !self.boundary.contains(point) {
            log::trace!("point is outside tree bounds, aborting");
            return Err(data);
        }

        if self.node_len() < Self::NODE_CAPACITY && self.is_leaf() {
            log::trace!("in leaf below capacity, adding to node");
            self.points.push(point);
            self.data.push(data);
            return Ok(());
        }

        if self.is_leaf() {
            log::trace!("subdividing node");
            self.subdivide();
        }

        let mut deque: VecDeque<&mut Self> = VecDeque::new();

        let children = [
            self.north_west.as_deref_mut(),
            self.north_east.as_deref_mut(),
            self.south_west.as_deref_mut(),
            self.south_east.as_deref_mut(),
        ];

        for child in children {
            if let Some(child) = child {
                deque.push_back(child);
            }
        }

        let mut data = Some(data);

        while let Some(node) = deque.pop_back() {
            if node.can_insert(point) {
                if node.is_leaf() {
                    if let Some(data_) = data {
                        // since we've already checked that the point
                        // can be inserted into the node, this
                        // recursive call won't actually do any
                        // further recursion
                        //
                        // lol
                        match node.insert(point, data_) {
                            Ok(_) => {
                                data = None;
                                break;
                            }
                            Err(d) => data = Some(d),
                        }
                    }
                } else {
                    if let Some(children) = node.children_mut() {
                        for child in children {
                            deque.push_back(child);
                        }
                    }
                }
            }
        }

        if let Some(data) = data {
            Err(data)
        } else {
            Ok(())
        }
    }

    pub fn query_range(&self, range: Rect) -> Vec<(Point, &T)> {
        let mut results = Vec::new();

        if !self.boundary.intersects(range) {
            return results;
        }

        for (point, data) in self.elems() {
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

    pub fn delete_nearest(&mut self, p: Point) -> bool {
        if let Some(closest) = self.nearest_mut(p) {
            closest.delete();
            true
        } else {
            false
        }
    }

    pub fn nearest_leaf(&self, tgt: Point) -> Option<&QuadTree<T>> {
        let mut best_dist: Option<f32> = None;
        let mut best_leaf: Option<&QuadTree<T>> = None;

        let rect_dist = |rect: Rect| {
            let l = rect.min().x;
            let r = rect.max().x;
            let t = rect.min().y;
            let b = rect.max().y;

            let l_ = (l - tgt.x).abs();
            let r_ = (r - tgt.x).abs();
            let t_ = (t - tgt.y).abs();
            let b_ = (b - tgt.y).abs();

            l_.min(r_).min(t_).min(b_)
        };

        let mut stack: VecDeque<&QuadTree<T>> = VecDeque::new();
        stack.push_back(self);

        while let Some(node) = stack.pop_back() {
            let bound = node.boundary();

            if let Some(dist) = best_dist {
                if (tgt.x < bound.min().x - dist)
                    || (tgt.x > bound.max().x + dist)
                    || (tgt.y > bound.min().y - dist)
                    || (tgt.y > bound.max().y + dist)
                {
                    //
                } else {
                    continue;
                }
            }

            if node.is_leaf() {
                // we know it's close enough to check

                for &point in node.points() {
                    let dist = point.dist(tgt);

                    if let Some(best) = best_dist {
                        if dist < best {
                            best_dist = Some(dist);
                            best_leaf = Some(node);
                        }
                    } else {
                        best_dist = Some(dist);
                        best_leaf = Some(node);
                    }
                }
            } else {
                if let Some(mut children) = node.children() {
                    children.sort_by(|a, b| {
                        let da = rect_dist(a.boundary());
                        let db = rect_dist(b.boundary());
                        da.partial_cmp(&db).unwrap()
                    });

                    for child in children {
                        stack.push_back(child);
                    }
                }
            }
        }

        best_leaf
    }

    pub fn nearest_leaf_mut(&mut self, tgt: Point) -> Option<&mut QuadTree<T>> {
        let mut best_dist: Option<f32> = None;
        let mut best_leaf: Option<&mut QuadTree<T>> = None;

        let rect_dist = |rect: Rect| {
            let l = rect.min().x;
            let r = rect.max().x;
            let t = rect.min().y;
            let b = rect.max().y;

            let l_ = (l - tgt.x).abs();
            let r_ = (r - tgt.x).abs();
            let t_ = (t - tgt.y).abs();
            let b_ = (b - tgt.y).abs();

            l_.min(r_).min(t_).min(b_)
        };

        let mut stack: VecDeque<&mut QuadTree<T>> = VecDeque::new();
        stack.push_back(self);

        while let Some(node) = stack.pop_back() {
            let bound = node.boundary();

            if let Some(dist) = best_dist {
                if (tgt.x < bound.min().x - dist)
                    || (tgt.x > bound.max().x + dist)
                    || (tgt.y > bound.min().y - dist)
                    || (tgt.y > bound.max().y + dist)
                {
                    //
                } else {
                    continue;
                }
            }

            if node.is_leaf() {
                // we know it's close enough to check

                let mut set_best = false;

                for &point in node.points() {
                    let dist = point.dist(tgt);

                    if let Some(best) = best_dist {
                        if dist < best {
                            best_dist = Some(dist);
                            set_best = true;
                        }
                    } else {
                        best_dist = Some(dist);
                        set_best = true;
                    }
                }
                if set_best {
                    best_leaf = Some(node);
                }
            } else {
                if let Some(mut children) = node.children_mut() {
                    children.sort_by(|a, b| {
                        let da = rect_dist(a.boundary());
                        let db = rect_dist(b.boundary());
                        da.partial_cmp(&db).unwrap()
                    });

                    for child in children {
                        stack.push_back(child);
                    }
                }
            }
        }

        best_leaf
    }

    pub fn nearest(&self, p: Point) -> Option<(Point, &T)> {
        let leaf = self.nearest_leaf(p)?;

        let mut closest: Option<(Point, &T)> = None;
        let mut prev_dist: Option<f32> = None;

        for (point, data) in leaf.elems() {
            let dist = point.dist_sqr(p);

            if let Some(prev) = prev_dist {
                if dist < prev {
                    closest = Some((point, data));
                    prev_dist = Some(dist);
                }
            } else {
                closest = Some((point, data));
                prev_dist = Some(dist);
            }
        }

        closest
    }

    pub fn nearest_mut(&mut self, p: Point) -> Option<PointMut<'_, T>> {
        let leaf = self.nearest_leaf_mut(p)?;

        let mut closest: Option<usize> = None;
        let mut prev_dist: Option<f32> = None;

        for (ix, &point) in leaf.points.iter().enumerate() {
            let dist = point.dist_sqr(p);

            if let Some(prev) = prev_dist {
                if dist < prev {
                    closest = Some(ix);
                    prev_dist = Some(dist);
                }
            } else {
                closest = Some(ix);
                prev_dist = Some(dist);
            }
        }

        let ix = closest?;
        let point = leaf.points[ix];

        Some(PointMut {
            node: leaf,
            ix,
            point,
        })
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

    pub fn leaves(&self) -> Leaves<'_, T> {
        Leaves::new(self)
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
}

pub struct PointMut<'a, T: Clone> {
    node: &'a mut QuadTree<T>,
    ix: usize,
    point: Point,
}

impl<'a, T: Clone> PointMut<'a, T> {
    pub fn point(&self) -> Point {
        self.point
    }

    pub fn data(&self) -> &T {
        &self.node.data[self.ix]
    }

    pub fn data_mut(&mut self) -> &mut T {
        &mut self.node.data[self.ix]
    }

    pub fn delete(self) {
        self.node.points.remove(self.ix);
        self.node.data.remove(self.ix);
    }
}

pub struct Leaves<'a, T: Clone> {
    stack: VecDeque<&'a QuadTree<T>>,
    done: bool,
}

impl<'a, T: Clone> Leaves<'a, T> {
    fn new(tree: &'a QuadTree<T>) -> Self {
        let mut stack = VecDeque::new();
        stack.push_back(tree);
        Self { stack, done: false }
    }

    fn next(&mut self) -> Option<&'a QuadTree<T>> {
        if self.done {
            return None;
        }

        while let Some(next) = self.stack.pop_back() {
            if next.is_leaf() {
                return Some(next);
            }

            if let Some(children) = next.children() {
                for child in children {
                    self.stack.push_back(child);
                }
            }
        }

        self.done = true;

        None
    }
}

impl<'a, T: Clone> Iterator for Leaves<'a, T> {
    type Item = &'a QuadTree<T>;

    fn next(&mut self) -> Option<Self::Item> {
        Leaves::next(self)
    }
}
