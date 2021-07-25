#[allow(unused_imports)]
use winit::{
    event::{self, Event, KeyboardInput, WindowEvent},
    event_loop::ControlFlow,
};

use rustc_hash::FxHashMap;

use crate::geometry::*;

pub trait InputPayload:
    Copy + PartialEq + Eq + PartialOrd + Ord + std::hash::Hash
{
}

impl<T> InputPayload for T where
    T: Copy + PartialEq + Eq + PartialOrd + Ord + std::hash::Hash
{
}

/// Trait for app subsystem inputs that can be bound to keys and other user input
pub trait BindableInput: InputPayload {
    fn default_binds() -> SystemInputBindings<Self>;
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum DigitalState {
    Pressed,
    Released,
}

impl DigitalState {
    pub fn pressed(&self) -> bool {
        *self == DigitalState::Pressed
    }

    pub fn released(&self) -> bool {
        *self == DigitalState::Released
    }
}

impl From<event::ElementState> for DigitalState {
    fn from(state: event::ElementState) -> Self {
        match state {
            event::ElementState::Pressed => Self::Pressed,
            event::ElementState::Released => Self::Released,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseButton<T: Copy + PartialEq> {
    // button: event::MouseButton,
    pos: Point,
    state: DigitalState,
    // modifiers: event::ModifiersState,
    payload: T,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct MouseWheel<T: Copy + PartialEq> {
    delta: f32,
    // modifiers: event::ModifiersState,
    payload: T,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct Keyboard<T: Copy + PartialEq> {
    // keycode: event::VirtualKeyCode,
    state: DigitalState,
    // modifiers: event::ModifiersState,
    payload: T,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MouseButtonBind<T: Copy + PartialEq> {
    // button: event::MouseButton,
    modifiers: event::ModifiersState,
    payload: T,
}

impl<T: Copy + PartialEq> MouseButtonBind<T> {
    pub fn new(payload: T) -> Self {
        Self {
            payload,
            modifiers: Default::default(),
        }
    }

    pub fn with_modifiers(
        payload: T,
        modifiers: event::ModifiersState,
    ) -> Self {
        Self { payload, modifiers }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct WheelBind<T: Copy + PartialEq> {
    invert: bool,
    mult: f32,
    modifiers: event::ModifiersState,
    payload: T,
}

impl<T: Copy + PartialEq> WheelBind<T> {
    pub fn new(invert: bool, mult: f32, payload: T) -> Self {
        Self {
            invert,
            mult,
            modifiers: Default::default(),
            payload,
        }
    }

    pub fn with_modifiers(
        invert: bool,
        mult: f32,
        payload: T,
        modifiers: event::ModifiersState,
    ) -> Self {
        Self {
            invert,
            mult,
            modifiers,
            payload,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct KeyBind<T: Copy + PartialEq> {
    // keycode: event::VirtualKeyCode,
    modifiers: event::ModifiersState,
    payload: T,
}

impl<T: Copy + PartialEq> KeyBind<T> {
    pub fn new(payload: T) -> Self {
        Self {
            payload,
            modifiers: Default::default(),
        }
    }

    pub fn with_modifiers(
        payload: T,
        modifiers: event::ModifiersState,
    ) -> Self {
        Self { payload, modifiers }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum SystemInput<T: InputPayload> {
    Keyboard {
        state: DigitalState,
        payload: T,
    },
    MouseButton {
        pos: Point,
        state: DigitalState,
        payload: T,
    },
    Wheel {
        delta: f32,
        payload: T,
    },
}

impl<T: InputPayload> SystemInput<T> {
    pub fn payload(&self) -> T {
        match self {
            SystemInput::Keyboard { payload, .. } => *payload,
            SystemInput::MouseButton { payload, .. } => *payload,
            SystemInput::Wheel { payload, .. } => *payload,
        }
    }

    pub fn is_keyboard(&self) -> bool {
        match self {
            SystemInput::Keyboard { .. } => true,
            SystemInput::MouseButton { .. } => false,
            SystemInput::Wheel { .. } => false,
        }
    }

    pub fn is_mouse(&self) -> bool {
        match self {
            SystemInput::Keyboard { .. } => false,
            SystemInput::MouseButton { .. } => true,
            SystemInput::Wheel { .. } => true,
        }
    }

    pub fn is_mouse_up(&self) -> bool {
        match self {
            SystemInput::Keyboard { .. } => false,
            SystemInput::MouseButton { state, .. } => state.released(),
            SystemInput::Wheel { .. } => false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct SystemInputBindings<Inputs>
where
    Inputs: InputPayload,
{
    key_binds: FxHashMap<event::VirtualKeyCode, Vec<KeyBind<Inputs>>>,
    mouse_binds: FxHashMap<event::MouseButton, Vec<MouseButtonBind<Inputs>>>,
    wheel_bind: Option<WheelBind<Inputs>>,
}

impl<Inputs: InputPayload> SystemInputBindings<Inputs> {
    pub fn new(
        key_binds: FxHashMap<event::VirtualKeyCode, Vec<KeyBind<Inputs>>>,
        mouse_binds: FxHashMap<
            event::MouseButton,
            Vec<MouseButtonBind<Inputs>>,
        >,
        wheel_bind: Option<WheelBind<Inputs>>,
    ) -> Self {
        Self {
            key_binds,
            mouse_binds,
            wheel_bind,
        }
    }

    pub fn apply(
        &self,
        // input_state: &mut InputState<Inputs>,
        event: &event::WindowEvent,
        modifiers: event::ModifiersState,
        mouse_pos: Point,
    ) -> Option<Vec<SystemInput<Inputs>>> {
        match event {
            // WindowEvent::ModifiersChanged(_) => {}
            // WindowEvent::CursorMoved { device_id, position, modifiers } => {}
            WindowEvent::KeyboardInput { input, .. } => {
                let key = input.virtual_keycode?;
                let state = DigitalState::from(input.state);

                let binds = self.key_binds.get(&key)?;

                let inputs = binds
                    .iter()
                    .filter(|&keybind| keybind.modifiers == modifiers)
                    .map(|&keybind| {
                        let payload = keybind.payload;
                        SystemInput::Keyboard { state, payload }
                    })
                    .collect::<Vec<_>>();

                // for &input in inputs.iter() {
                // input_state.update(input);
                // }

                Some(inputs)
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let binds = self.mouse_binds.get(&button)?;
                let state = DigitalState::from(*state);

                let inputs = binds
                    .iter()
                    .filter(|&mousebind| mousebind.modifiers == modifiers)
                    .map(|&mousebind| {
                        let payload = mousebind.payload;
                        SystemInput::MouseButton {
                            pos: mouse_pos,
                            state,
                            payload,
                        }
                    })
                    .collect::<Vec<_>>();

                // for &input in inputs.iter() {
                // input_state.update(input);
                // }

                Some(inputs)
            }
            WindowEvent::MouseWheel {
                delta,
                phase: _phase,
                ..
            } => {
                if let Some(bind) = self.wheel_bind {
                    if bind.modifiers != modifiers {
                        return None;
                    }

                    let mut mult = bind.mult;
                    if bind.invert {
                        mult *= -1.0;
                    }

                    let delta = match delta {
                        event::MouseScrollDelta::LineDelta(_x, y) => {
                            // eprintln!("LineDelta({}, {}", x, y);
                            *y
                        }
                        event::MouseScrollDelta::PixelDelta(pos) => {
                            // eprintln!("PixelDelta({:.4}, {:.4})", pos.x, pos.y);

                            // PixelDelta events seem to differ in
                            // frequency as well as magnitude; a more
                            // complex solution is probably needed
                            (pos.y as f32) * 0.1
                        }
                    };

                    Some(vec![SystemInput::Wheel {
                        delta: delta * mult,
                        payload: bind.payload,
                    }])
                } else {
                    None
                }
            }
            _ => None,
        }
    }
}
