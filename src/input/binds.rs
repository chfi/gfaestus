#[allow(unused_imports)]
use winit::{
    event::{self, Event, KeyboardInput, WindowEvent},
    event_loop::ControlFlow,
};

use std::thread;

use crossbeam::channel;

use crate::geometry::*;

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum InputChange {
    Pressed,
    Released,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum DigitalInputKind {
    Single,
    // Double,
    Held,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseButton {
    button: event::MouseButton,
    pos: Point,
    state: InputChange,
    modifiers: event::ModifiersState,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct MouseWheel {
    delta: f32,
    modifiers: event::ModifiersState,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Keyboard {
    keycode: event::VirtualKeyCode,
    state: InputChange,
    modifiers: event::ModifiersState,
}

pub struct MouseButtonBind {
    button: event::MouseButton,
    modifiers: event::ModifiersState,
    digital_kind: DigitalInputKind,
}

pub struct WheelBind {
    invert: bool,
    mult: f32,
    modifiers: event::ModifiersState,
}

pub struct KeyBind {
    keycode: event::VirtualKeyCode,
    state: InputChange,
    modifiers: event::ModifiersState,
    digital_kind: DigitalInputKind,
}

// pub trait InputBinding {
//     type SystemInputs: Sized + Clone + PartialEq;
// }

// pub trait SystemInputs {
//     type Bindings: InputBinding;
// }
