#[allow(unused_imports)]
use winit::{
    event::{self, Event, KeyboardInput, WindowEvent},
    event_loop::ControlFlow,
};

use std::thread;

use crossbeam::channel;

use rustc_hash::{FxHashMap, FxHashSet};

use crate::geometry::*;

pub trait InputPayload:
    Copy + PartialEq + Eq + PartialOrd + Ord + std::hash::Hash
{
}

impl<T> InputPayload for T where
    T: Copy + PartialEq + Eq + PartialOrd + Ord + std::hash::Hash
{
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
    // modifiers: event::ModifiersState,
    payload: T,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct WheelBind<T: Copy + PartialEq> {
    invert: bool,
    mult: f32,
    // modifiers: event::ModifiersState,
    payload: T,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub struct KeyBind<T: Copy + PartialEq> {
    // keycode: event::VirtualKeyCode,
    // modifiers: event::ModifiersState,
    payload: T,
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum SystemInput<T: InputPayload> {
    Keyboard { state: DigitalState, payload: T },
    MouseButton { state: DigitalState, payload: T },
    Wheel { delta: f32, payload: T },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MainViewInputs {
    ButtonMousePan,
    ButtonSelect,
    KeyClearSelection,
    KeyPanUp,
    KeyPanRight,
    KeyPanDown,
    KeyPanLeft,
    KeyResetView,
    WheelZoom,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SystemInputBindings<Inputs>
where
    Inputs: InputPayload,
{
    key_binds: FxHashMap<event::VirtualKeyCode, Vec<KeyBind<Inputs>>>,
    mouse_binds: FxHashMap<event::MouseButton, Vec<MouseButtonBind<Inputs>>>,
    wheel_bind: WheelBind<Inputs>,
}

impl<Inputs: InputPayload> SystemInputBindings<Inputs> {
    pub fn apply(
        &self,
        event: &event::WindowEvent,
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
                    .map(|&keybind| {
                        let payload = keybind.payload;
                        SystemInput::Keyboard { state, payload }
                    })
                    .collect::<Vec<_>>();

                Some(inputs)
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let binds = self.mouse_binds.get(&button)?;
                let state = DigitalState::from(*state);

                let inputs = binds
                    .iter()
                    .map(|&mousebind| {
                        let payload = mousebind.payload;
                        SystemInput::MouseButton { state, payload }
                    })
                    .collect::<Vec<_>>();

                Some(inputs)
            }
            WindowEvent::MouseWheel {
                delta,
                phase: _phase,
                ..
            } => {
                let mut mult = self.wheel_bind.mult;
                if self.wheel_bind.invert {
                    mult *= -1.0;
                }

                let delta = match delta {
                    event::MouseScrollDelta::LineDelta(_, y) => *y,
                    event::MouseScrollDelta::PixelDelta(pos) => pos.y as f32,
                };

                Some(vec![SystemInput::Wheel {
                    delta: delta * mult,
                    payload: self.wheel_bind.payload,
                }])
            }
            _ => None,
        }
    }
}

impl std::default::Default for SystemInputBindings<MainViewInputs> {
    fn default() -> Self {
        use event::VirtualKeyCode as Key;
        use MainViewInputs as Inputs;

        let key_binds: FxHashMap<event::VirtualKeyCode, Vec<KeyBind<Inputs>>> =
            [
                (Key::Up, Inputs::KeyPanUp),
                (Key::Down, Inputs::KeyPanDown),
                (Key::Left, Inputs::KeyPanLeft),
                (Key::Right, Inputs::KeyPanRight),
                (Key::Escape, Inputs::KeyClearSelection),
                (Key::Space, Inputs::KeyResetView),
            ]
            .iter()
            .copied()
            .map(|(k, i)| (k, vec![KeyBind { payload: i }]))
            .collect::<FxHashMap<_, _>>();

        let mouse_binds: FxHashMap<
            event::MouseButton,
            Vec<MouseButtonBind<Inputs>>,
        > = [(
            event::MouseButton::Left,
            vec![
                MouseButtonBind {
                    payload: Inputs::ButtonMousePan,
                },
                MouseButtonBind {
                    payload: Inputs::ButtonSelect,
                },
            ],
        )]
        .iter()
        .cloned()
        .collect();

        let wheel_bind = WheelBind {
            invert: false,
            mult: 0.45,
            payload: Inputs::WheelZoom,
        };

        Self {
            key_binds,
            mouse_binds,
            wheel_bind,
        }
    }
}
