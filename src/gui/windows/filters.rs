#[allow(unused_imports)]
use handlegraph::{
    handle::{Direction, Handle, NodeId},
    handlegraph::*,
    mutablehandlegraph::*,
    packed::*,
    packedgraph::index::OneBasedIndex,
    packedgraph::*,
    path_position::*,
    pathhandlegraph::*,
};

use bstr::ByteSlice;

use egui::emath::Numeric;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum FilterStringOp {
    None,
    Equal,
    Contains,
    ContainedIn,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct FilterString {
    pub op: FilterStringOp,
    pub arg: String,
}

impl std::default::Default for FilterString {
    fn default() -> Self {
        Self {
            op: FilterStringOp::None,
            arg: String::new(),
        }
    }
}

impl FilterString {
    pub fn filter_str(&self, string: &str) -> bool {
        match self.op {
            FilterStringOp::None => true,
            FilterStringOp::Equal => string == self.arg,
            FilterStringOp::Contains => string.contains(&self.arg),
            FilterStringOp::ContainedIn => self.arg.contains(string),
        }
    }

    pub fn filter_bytes(&self, string: &[u8]) -> bool {
        match self.op {
            FilterStringOp::None => true,
            FilterStringOp::Equal => {
                let bytes = self.arg.as_bytes();
                string == bytes
            }
            FilterStringOp::Contains => {
                let bytes = self.arg.as_bytes();
                string.contains_str(bytes)
            }
            FilterStringOp::ContainedIn => {
                let bytes = self.arg.as_bytes();
                bytes.contains_str(string)
            }
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) -> Option<egui::Response> {
        let op = &mut self.op;
        let arg = &mut self.arg;

        ui.horizontal(|ui| {
            let _op_none = ui.radio_value(op, FilterStringOp::None, "None");
            let _op_equal = ui.radio_value(op, FilterStringOp::Equal, "Equal");
            let _op_contains =
                ui.radio_value(op, FilterStringOp::Contains, "Contains");
            let _op_contained_in =
                ui.radio_value(op, FilterStringOp::ContainedIn, "Contained in");
        });

        if *op != FilterStringOp::None {
            let arg_edit = ui.text_edit_singleline(arg);
            return Some(arg_edit);
        }

        None
    }
}

#[derive(Debug, PartialEq, PartialOrd)]
pub enum FilterNumOp {
    None,
    Equal,
    LessThan,
    MoreThan,
    InRange,
}

#[derive(Debug, PartialEq, PartialOrd)]
pub struct FilterNum<T: Numeric> {
    pub op: FilterNumOp,
    pub arg1: T,
    pub arg2: T,
}

impl<T: Numeric> std::default::Default for FilterNum<T> {
    fn default() -> Self {
        Self {
            op: FilterNumOp::None,
            arg1: T::from_f64(0.0),
            arg2: T::from_f64(0.0),
        }
    }
}

impl<T: Numeric> FilterNum<T> {
    pub fn filter(&self, val: T) -> bool {
        match self.op {
            FilterNumOp::None => true,
            FilterNumOp::Equal => val == self.arg1,
            FilterNumOp::LessThan => val < self.arg1,
            FilterNumOp::MoreThan => val > self.arg1,
            FilterNumOp::InRange => self.arg1 <= val && val < self.arg2,
        }
    }

    pub fn ui(&mut self, ui: &mut egui::Ui) {
        let op = &mut self.op;
        let arg1 = &mut self.arg1;
        let arg2 = &mut self.arg2;

        ui.horizontal(|ui| {
            let _op_none = ui.radio_value(op, FilterNumOp::None, "None");
            let _op_equal = ui.radio_value(op, FilterNumOp::Equal, "Equal");
            let _op_less =
                ui.radio_value(op, FilterNumOp::LessThan, "Less than");
            let _op_more =
                ui.radio_value(op, FilterNumOp::MoreThan, "More than");
            let _op_in_range =
                ui.radio_value(op, FilterNumOp::InRange, "In range");
        });

        let arg1_drag = egui::DragValue::new::<T>(arg1);
        // egui::DragValue::new::<T>(from_pos).clamp_range(from_range);

        let arg2_drag = egui::DragValue::new::<T>(arg2);

        if *op != FilterNumOp::None {
            ui.horizontal(|ui| {
                let _arg1_edit = ui.add(arg1_drag);
                if *op == FilterNumOp::InRange {
                    let _arg2_edit = ui.add(arg2_drag);
                }
            });
        }
    }
}
