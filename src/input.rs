use winit::event::{Event, KeyboardInput, WindowEvent};
use winit::event_loop::{ControlFlow, EventLoop};
use winit::window::{Window, WindowBuilder};

use winit::event;

use std::sync::Arc;

use std::time::Instant;

use crossbeam::channel;

use crate::geometry::*;
use crate::gfa::*;
use crate::view;
use crate::view::View;

use crate::ui::UICmd;

pub enum InputEvent {
    KeyboardInput(event::KeyboardInput),
    MouseInput(event::MouseButton, InputChange),
    MouseWheel(event::MouseScrollDelta),
    CursorMoved(Point),
    // CursorEntered(event::CursorEntered),
    // CursorLeft(event::CursorLeft),
}

impl InputEvent {
    pub fn from_window_event(win_event: &WindowEvent<'_>) -> Option<InputEvent> {
        use WindowEvent as WinEvent;
        match win_event {
            WinEvent::KeyboardInput { input, .. } => Some(InputEvent::KeyboardInput(*input)),
            WinEvent::MouseInput { button, state, .. } => {
                let input_change = match state {
                    winit::event::ElementState::Pressed => InputChange::Pressed,
                    winit::event::ElementState::Released => InputChange::Released,
                };
                Some(InputEvent::MouseInput(*button, input_change))
            }
            WinEvent::MouseWheel { delta, .. } => Some(InputEvent::MouseWheel(*delta)),
            WinEvent::CursorMoved { position, .. } => {
                let x = position.x as f32;
                let y = position.y as f32;
                Some(InputEvent::CursorMoved(Point { x, y }))
            }
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum InputChange {
    Pressed,
    Released,
}

impl InputChange {
    pub fn pressed(&self) -> bool {
        self == InputChange::Pressed
    }

    pub fn released(&self) -> bool {
        self == InputChange::Released
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum SemanticInput {
    KeyPanUp(InputChange),
    KeyPanRight(InputChange),
    KeyPanDown(InputChange),
    KeyPanLeft(InputChange),
    KeyPause(InputChange),
    KeyReset(InputChange),
    MouseButtonPan(InputChange),
    MouseZoomDelta(f32),
    MouseCursorPos(Point),
}

pub struct RawInputHandler {
    event_rx: channel::Receiver<InputEvent>,
    event_tx: channel::Sender<InputEvent>,
    semantic_rx: channel::Receiver<SemanticInput>,
    semantic_tx: channel::Sender<SemanticInput>,
}

impl RawInputHandler {
    pub fn new() -> Self {
        let (event_tx, event_rx) = channel::unbounded::<InputEvent>();
        let (semantic_tx, semantic_rx) = channel::unbounded::<SemanticInput>();

        Self {
            event_tx,
            event_rx,
            semantic_rx,
            semantic_tx,
        }
    }

    pub fn parse_window_event(&self, event: &WindowEvent) {
        if let Some(sem_event) = InputEvent::from_window_event(event) {
            use SemanticInput as SemIn;

            match sem_event {
                InputEvent::KeyboardInput(input) => {
                    use winit::event::VirtualKeyCode as Key;

                    let pressed = input.state == winit::event::ElementState::Pressed;
                    let input_change = if pressed {
                        InputChange::Pressed
                    } else {
                        InputChange::Released
                    };

                    if let Some(key) = input.virtual_keycode {
                        let semantic_input = match key {
                            Key::Up => Some(SemIn::KeyPanUp(input_change)),
                            Key::Right => Some(SemIn::KeyPanRight(input_change)),
                            Key::Down => Some(SemIn::KeyPanDown(input_change)),
                            Key::Left => Some(SemIn::KeyPanLeft(input_change)),
                            _ => None,
                        };

                        if let Some(sem_in) = semantic_input {
                            self.semantic_tx.send(sem_in).unwrap();
                        }
                    }
                }
                InputEvent::MouseInput(button, input_change) => {
                    let to_send = match button {
                        event::MouseButton::Left => Some(SemIn::MouseButtonPan(input_change)),
                        _ => None,
                        // event::MouseButton::Right => {}
                        // event::MouseButton::Middle => {}
                        // event::MouseButton::Other(_) => {}
                    };

                    if let Some(sem_in) = to_send {
                        self.semantic_tx.send(sem_in).unwrap();
                    }
                }
                InputEvent::MouseWheel(delta) => {
                    use winit::event::MouseScrollDelta as ScrollDelta;
                    let to_send = match delta {
                        ScrollDelta::LineDelta(_x, y) => {
                            if y > 0.0 {
                                Some(SemIn::MouseZoomDelta(-0.45))
                            } else if y < 0.0 {
                                Some(SemIn::MouseZoomDelta(0.45))
                            } else {
                                None
                            }
                        }
                        ScrollDelta::PixelDelta(_pos) => None,
                    };

                    if let Some(sem_in) = to_send {
                        self.semantic_tx.send(sem_in).unwrap();
                    }
                }
                InputEvent::CursorMoved(pos) => {
                    self.semantic_tx.send(SemIn::MouseCursorPos(pos)).unwrap();
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct SemanticInputHandler {
    key_pan_up: bool,
    key_pan_right: bool,
    key_pan_down: bool,
    key_pan_left: bool,
    mouse_pan: Option<Point>,
    mouse_pos: Point,
    mouse_pan_mod_down: bool,
}
