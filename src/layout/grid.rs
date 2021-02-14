use crate::geometry::*;

use nalgebra_glm as glm;

use anyhow::{Context, Result};

// pub type EntryVal

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellDims {
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridDims {
    pub columns: usize,
    pub rows: usize,
}

impl GridDims {
    #[inline]
    pub fn cell_count(&self) -> usize {
        self.columns * self.rows
    }

    #[inline]
    pub fn as_point(&self) -> Point {
        Point {
            x: self.columns as f32,
            y: self.rows as f32,
        }
    }
}

pub trait EntryVal: Copy + Sized + PartialEq {}

impl<T> EntryVal for T where T: Copy + Sized + PartialEq {}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
// pub struct Entry<T: Copy + Sized + PartialEq> {
pub struct Entry<T: EntryVal> {
    point: Point,
    value: T,
}

impl<T: EntryVal> Entry<T> {
    #[inline]
    pub fn new(point: Point, value: T) -> Self {
        Self { point, value }
    }
}

#[derive(Debug, Clone)]
pub struct Cell<T: EntryVal> {
    entries: Vec<Entry<T>>,
    top_left: Point,
    dims: CellDims,
    // id: usize,
}

impl<T: EntryVal> Cell<T> {
    #[inline]
    pub fn grid_to_local(&self, point: Point) -> Point {
        point - self.top_left
    }

    #[inline]
    pub fn local_to_grid(&self, point: Point) -> Point {
        point + self.top_left
    }

    #[inline]
    pub fn grid_to_local_norm(&self, point: Point) -> Point {
        let p = point - self.top_left;
        Point {
            x: p.x / self.dims.width,
            y: p.y / self.dims.height,
        }
    }

    #[inline]
    pub fn new(top_left: Point, dims: CellDims) -> Self {
        Self {
            entries: Vec::new(),
            top_left,
            dims,
        }
    }

    #[inline]
    pub fn with_capacity(top_left: Point, dims: CellDims, cap: usize) -> Self {
        Self {
            entries: Vec::with_capacity(cap),
            top_left,
            dims,
        }
    }

    #[inline]
    pub fn new_entry(&mut self, point: Point, value: T) {
        let find_index = self
            .entries
            .binary_search_by(|e| e.point.partial_cmp(&point).unwrap());

        let index = match find_index {
            Ok(x) => x,
            Err(x) => x,
        };

        self.entries.insert(index, Entry::new(point, value))
    }

    #[inline]
    pub fn find_in_rect(
        &self,
        top_left: Point,
        bottom_right: Point,
    ) -> impl Iterator<Item = Entry<T>> + '_ {
        let x_range = {
            let start_ix = self
                .entries
                .binary_search_by(|e| e.point.x.partial_cmp(&top_left.x).unwrap())
                .map_or_else(|x| x, |x| x);

            let greater = &self.entries[start_ix..];

            let end_ix = greater
                .binary_search_by(|e| e.point.x.partial_cmp(&bottom_right.x).unwrap())
                .map_or_else(|x| x, |x| x);
            &self.entries[..end_ix]
        };

        x_range.iter().filter_map(move |&e| {
            if e.point.y >= top_left.y && e.point.y <= bottom_right.y {
                Some(e)
            } else {
                None
            }
        })
    }

    #[inline]
    pub fn find_in_x_radius(&self, x: f32, radius: f32) -> &[Entry<T>] {
        let min_x = x - radius;
        let max_x = x + radius;

        if max_x < self.top_left.x || max_x > self.top_left.x + self.dims.width {
            return &self.entries[0..0];
        }

        let greater = if min_x > self.top_left.x {
            let start_ix = self
                .entries
                .binary_search_by(|e| e.point.x.partial_cmp(&min_x).unwrap())
                .map_or_else(|x| x, |x| x);
            &self.entries[start_ix..]
        } else {
            &self.entries[..]
        };

        let range = if max_x < self.top_left.x + self.dims.width {
            let end_ix = greater
                .binary_search_by(|e| e.point.x.partial_cmp(&max_x).unwrap())
                .map_or_else(|x| x, |x| x);
            &self.entries[..end_ix]
        } else {
            &self.entries[..]
        };

        range
    }

    #[inline]
    pub fn find_in_radius(&self, point: Point, radius: f32) -> impl Iterator<Item = Entry<T>> + '_ {
        let x_range = self.find_in_x_radius(point.x, radius);

        x_range.iter().filter_map(move |&e| {
            if e.point.dist(point) <= radius {
                Some(e)
            } else {
                None
            }
        })
    }
}

#[derive(Debug, Clone)]
pub struct Grid<T: EntryVal> {
    top_left: Point,
    cell_dims: CellDims,
    grid_dims: GridDims,
    cells: Vec<Cell<T>>,
}

impl<T: EntryVal> Grid<T> {
    pub fn new(top_left: Point, grid_dims: GridDims, cell_dims: CellDims) -> Self {
        let mut cells: Vec<Cell<T>> = Vec::with_capacity(grid_dims.cell_count());

        for column in 0..grid_dims.columns {
            for row in 0..grid_dims.rows {
                let x = (column as f32) * cell_dims.width;
                let y = (row as f32) * cell_dims.height;

                let cell_top_left = top_left + Point { x, y };
                let cell = Cell::new(cell_top_left, cell_dims);

                cells.push(cell);
            }
        }

        Self {
            top_left,
            cell_dims,
            grid_dims,
            cells,
        }
    }

    #[inline]
    fn world_unit_dims(&self) -> Point {
        let width = self.cell_dims.width * self.grid_dims.columns as f32;
        let height = self.cell_dims.height * self.grid_dims.rows as f32;
        Point {
            x: width,
            y: height,
        }
    }

    #[inline]
    pub fn world_rect(&self) -> (Point, Point) {
        let p0 = self.top_left;
        let p1 = p0 + self.world_unit_dims();
        (p0, p1)
    }

    #[inline]
    pub fn cell_col_row_at_point(&self, point: Point) -> Option<(usize, usize)> {
        let (top_left, bottom_right) = self.world_rect();
        if point.x < top_left.x
            || point.y < top_left.y
            || point.x > bottom_right.x
            || point.y > bottom_right.y
        {
            return None;
        }

        let dims = self.world_unit_dims();

        let column = (point.x / dims.x) as usize;
        let row = (point.y / dims.y) as usize;

        Some((column, row))
    }

    #[inline]
    pub fn cell_index_at_point(&self, point: Point) -> Option<usize> {
        let (top_left, bottom_right) = self.world_rect();
        if point.x < top_left.x
            || point.y < top_left.y
            || point.x > bottom_right.x
            || point.y > bottom_right.y
        {
            return None;
        }

        let dims = self.world_unit_dims();

        let column = (point.x / dims.x) as usize;
        let row = (point.y / dims.y) as usize;

        Some((row / self.grid_dims.columns) + (column % self.grid_dims.columns))
    }

    #[inline]
    pub fn cell_index(&self, column: usize, row: usize) -> Option<usize> {
        if column >= self.grid_dims.columns || row >= self.grid_dims.rows {
            return None;
        }
        Some((row / self.grid_dims.columns) + (column % self.grid_dims.columns))
    }

    /// Provided rectangle does *not* have to be fully contained in
    /// the grid, or at all -- any cells that have any overlap with
    /// the rect are returned
    #[inline]
    pub fn cell_indices_in_world_rect(&self, p0: Point, p1: Point) -> Vec<usize> {
        let dims = self.world_unit_dims();

        let min_x = p0.x.min(p1.x);
        let max_x = p0.x.max(p1.x);

        let min_y = p0.y.min(p1.y);
        let max_y = p0.y.max(p1.y);

        let left = (min_x >= 0.0)
            .then(|| (min_x / dims.x) as usize)
            .unwrap_or(0);
        let top = (min_y >= 0.0)
            .then(|| (min_y / dims.y) as usize)
            .unwrap_or(0);

        let right = (max_x < dims.x)
            .then(|| (max_x / dims.x) as usize)
            .unwrap_or(self.grid_dims.columns - 1);
        let bottom = (max_y < dims.y)
            .then(|| (max_y / dims.y) as usize)
            .unwrap_or(self.grid_dims.rows - 1);

        let mut indices = Vec::with_capacity((right - left) * (bottom - top));

        for col in left..=right {
            for row in top..=bottom {
                let ix = (row / self.grid_dims.columns) + (col % self.grid_dims.columns);
                indices.push(ix);
            }
        }

        indices
    }
}

impl<T: EntryVal> Grid<T> {
    #[inline]
    pub fn collect_in_rect(&self, p0: Point, p1: Point) -> Vec<Entry<T>> {
        let cell_indices = self.cell_indices_in_world_rect(p0, p1);

        let mut res = Vec::new();

        for cell_ix in cell_indices {
            let cell = &self.cells[cell_ix];
            res.extend(cell.find_in_rect(p0, p1));
        }

        res
    }

    // #[inline]
    // pub fn collect_in_radius(&self, point: Point, radius: f32) -> Vec<Entry<T>> {
    //     let mut res = Vec::new();
    // }
}
