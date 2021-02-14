use crate::geometry::*;

use nalgebra_glm as glm;

use anyhow::{Context, Result};

// pub type EntryVal

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CellDims {
    width: f32,
    height: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridDims {
    columns: usize,
    rows: usize,
}

impl GridDims {
    #[inline]
    pub fn cell_count(&self) -> usize {
        self.columns * self.rows
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
}
