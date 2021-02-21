#[allow(unused_imports)]
use winit::{
    event::{self, Event, KeyboardInput, WindowEvent},
    event_loop::ControlFlow,
};

use std::thread;

use crossbeam::channel;

use crate::geometry::*;

#[derive(Debug, Clone, Copy)]
pub enum InputEvent {
    KeyboardInput(event::KeyboardInput),
    MouseInput(event::MouseButton, InputChange),
    MouseWheel(event::MouseScrollDelta),
    CursorMoved(Point),
    // CursorEntered(event::CursorEntered),
    // CursorLeft(event::CursorLeft),
}

impl InputEvent {
    pub fn from_window_event(
        win_event: &WindowEvent<'_>,
    ) -> Option<InputEvent> {
        use WindowEvent as WinEvent;
        match win_event {
            WinEvent::KeyboardInput { input, .. } => {
                Some(InputEvent::KeyboardInput(*input))
            }
            WinEvent::MouseInput { button, state, .. } => {
                let input_change = match state {
                    winit::event::ElementState::Pressed => InputChange::Pressed,
                    winit::event::ElementState::Released => {
                        InputChange::Released
                    }
                };
                Some(InputEvent::MouseInput(*button, input_change))
            }
            WinEvent::MouseWheel { delta, .. } => {
                Some(InputEvent::MouseWheel(*delta))
            }
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
        *self == InputChange::Pressed
    }

    pub fn released(&self) -> bool {
        *self == InputChange::Released
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
    OtherKey {
        key: winit::event::VirtualKeyCode,
        pressed: bool,
    },
}

impl SemanticInput {
    pub fn parse_input_event(in_event: InputEvent) -> Option<Self> {
        use SemanticInput as SemIn;

        match in_event {
            InputEvent::KeyboardInput(input) => {
                use winit::event::VirtualKeyCode as Key;

                let pressed =
                    input.state == winit::event::ElementState::Pressed;
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
                        Key::Space => Some(SemIn::KeyPause(input_change)),
                        Key::Return => Some(SemIn::KeyReset(input_change)),
                        x => Some(SemIn::OtherKey { key: x, pressed }),
                    };

                    return semantic_input;
                }

                None
            }
            InputEvent::MouseInput(button, input_change) => {
                let to_send = match button {
                    event::MouseButton::Left => {
                        Some(SemIn::MouseButtonPan(input_change))
                    }
                    _ => None,
                    // event::MouseButton::Right => {}
                    // event::MouseButton::Middle => {}
                    // event::MouseButton::Other(_) => {}
                };

                return to_send;
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

                return to_send;
            }
            InputEvent::CursorMoved(pos) => {
                return Some(SemIn::MouseCursorPos(pos));
            }
        }
    }

    pub fn parse_window_event(event: &WindowEvent) -> Option<Self> {
        let in_event = InputEvent::from_window_event(event)?;
        Self::parse_input_event(in_event)
    }
}
pub struct SemanticInputWorker {
    _worker_thread: thread::JoinHandle<()>,
    semantic_input_rx: channel::Receiver<SemanticInput>,
    input_tx: channel::Sender<InputEvent>,
}

impl SemanticInputWorker {
    pub fn send_window_event(&self, win_event: &WindowEvent<'_>) {
        if let Some(in_event) = InputEvent::from_window_event(win_event) {
            self.input_tx.send(in_event).unwrap();
        }
    }

    pub fn clone_semantic_rx(&self) -> channel::Receiver<SemanticInput> {
        self.semantic_input_rx.clone()
    }
}

pub fn input_event_handler() -> SemanticInputWorker {
    let (input_tx, input_rx) = channel::unbounded::<InputEvent>();

    let (semantic_input_tx, semantic_input_rx) =
        channel::unbounded::<SemanticInput>();

    let _worker_thread = thread::spawn(move || {
        while let Ok(in_event) = input_rx.recv() {
            let semantic = SemanticInput::parse_input_event(in_event);

            if let Some(sem_ev) = semantic {
                semantic_input_tx.send(sem_ev).unwrap();
            }
        }
    });

    SemanticInputWorker {
        _worker_thread,
        semantic_input_rx,
        input_tx,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum InputAction {
    KeyPan {
        up: bool,
        right: bool,
        down: bool,
        left: bool,
    },
    PausePhysics,
    ResetLayout,
    MousePan(Option<Point>),
    MouseZoom {
        focus: Point,
        delta: f32,
    },
    MouseAt {
        point: Point,
    },
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct SemanticInputState {
    key_pan_up: bool,
    key_pan_right: bool,
    key_pan_down: bool,
    key_pan_left: bool,
    mouse_pan: Option<Point>,
    mouse_pos: Point,
    mouse_pan_mod_down: bool,
}

impl SemanticInputState {
    fn key_pan_action(&self) -> InputAction {
        InputAction::KeyPan {
            up: self.key_pan_up,
            right: self.key_pan_right,
            down: self.key_pan_down,
            left: self.key_pan_left,
        }
    }

    fn apply_sem_input(&mut self, input: SemanticInput) -> Option<InputAction> {
        use SemanticInput as SemIn;

        match input {
            SemIn::KeyPanUp(state) => {
                self.key_pan_up = state.pressed();
                Some(self.key_pan_action())
            }
            SemIn::KeyPanRight(state) => {
                self.key_pan_right = state.pressed();
                Some(self.key_pan_action())
            }
            SemIn::KeyPanDown(state) => {
                self.key_pan_down = state.pressed();
                Some(self.key_pan_action())
            }
            SemIn::KeyPanLeft(state) => {
                self.key_pan_left = state.pressed();
                Some(self.key_pan_action())
            }
            SemIn::KeyPause(InputChange::Pressed) => {
                Some(InputAction::PausePhysics)
            }
            SemIn::KeyReset(InputChange::Pressed) => {
                Some(InputAction::ResetLayout)
            }
            SemIn::MouseButtonPan(state) => {
                use InputChange::{Pressed, Released};
                match (state, self.mouse_pan) {
                    (Pressed, None) => {
                        let focus = self.mouse_pos;
                        self.mouse_pan = Some(focus);
                        Some(InputAction::MousePan(Some(focus)))
                    }
                    (Released, None) => None,
                    (Pressed, Some(_)) => None,
                    (Released, Some(_pos)) => {
                        self.mouse_pan = None;
                        Some(InputAction::MousePan(None))
                    }
                }
            }
            SemIn::MouseZoomDelta(delta) => Some(InputAction::MouseZoom {
                focus: self.mouse_pos,
                delta,
            }),
            SemIn::MouseCursorPos(pos) => {
                self.mouse_pos = pos;
                Some(InputAction::MouseAt { point: pos })
            }
            _ => None,
        }
    }
}

pub struct InputActionWorker {
    _worker_thread: thread::JoinHandle<()>,
    raw_event_tx: channel::Sender<InputEvent>,
    semantic_input_rx: channel::Receiver<SemanticInput>,
    input_action_rx: channel::Receiver<InputAction>,
}

impl InputActionWorker {
    pub fn new() -> Self {
        let (raw_event_tx, raw_event_rx) = channel::unbounded::<InputEvent>();
        let (semantic_input_tx, semantic_input_rx) =
            channel::unbounded::<SemanticInput>();
        let (input_action_tx, input_action_rx) =
            channel::unbounded::<InputAction>();

        let mut input_state = SemanticInputState {
            key_pan_up: false,
            key_pan_right: false,
            key_pan_down: false,
            key_pan_left: false,
            mouse_pan: None,
            mouse_pos: Point::default(),
            mouse_pan_mod_down: false,
        };

        let _worker_thread = thread::spawn(move || {
            while let Ok(in_event) = raw_event_rx.recv() {
                if let Some(sem_ev) = SemanticInput::parse_input_event(in_event)
                {
                    semantic_input_tx.send(sem_ev).unwrap();

                    let input_action = input_state.apply_sem_input(sem_ev);

                    if let Some(action) = input_action {
                        input_action_tx.send(action).unwrap();
                    }
                }
            }
        });

        InputActionWorker {
            _worker_thread,
            raw_event_tx,
            semantic_input_rx,
            input_action_rx,
        }
    }

    pub fn send_window_event(&self, win_event: &WindowEvent<'_>) {
        if let Some(in_event) = InputEvent::from_window_event(win_event) {
            self.raw_event_tx.send(in_event).unwrap();
        }
    }

    pub fn clone_semantic_rx(&self) -> channel::Receiver<SemanticInput> {
        self.semantic_input_rx.clone()
    }

    pub fn clone_action_rx(&self) -> channel::Receiver<InputAction> {
        self.input_action_rx.clone()
    }
}
