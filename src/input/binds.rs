#[allow(unused_imports)]
use winit::{
    event::{self, Event, KeyboardInput, WindowEvent},
    event_loop::ControlFlow,
};

use std::thread;

use crossbeam::atomic::AtomicCell;
use crossbeam::channel;
use std::sync::Arc;

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
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct InputState<T: InputPayload> {
    keys: FxHashSet<T>,
    mouse_buttons: FxHashSet<T>,
}

impl<T: InputPayload> InputState<T> {
    pub fn is_key_down(&self, key_input: T) -> bool {
        self.keys.contains(&key_input)
    }

    pub fn is_mouse_down(&self, mouse_input: T) -> bool {
        self.mouse_buttons.contains(&mouse_input)
    }

    pub fn update(&mut self, input: SystemInput<T>) {
        match input {
            SystemInput::Keyboard { state, payload } => {
                if state.pressed() {
                    self.keys.insert(payload);
                } else {
                    self.keys.remove(&payload);
                }
            }
            SystemInput::MouseButton { state, payload, .. } => {
                if state.pressed() {
                    self.mouse_buttons.insert(payload);
                } else {
                    self.mouse_buttons.remove(&payload);
                }
            }
            _ => (),
        }
    }
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum GuiInput {
    KeyClearSelection,
    KeyEguiInspectionUi,
    KeyEguiSettingsUi,
    KeyEguiMemoryUi,
    // KeyGfaestusSettingsUi,
    ButtonLeft,
    ButtonRight,
    WheelScroll,
}

struct InputChannels<T: InputPayload> {
    tx: channel::Sender<SystemInput<T>>,
    rx: channel::Receiver<SystemInput<T>>,
}

// struct InputManagerThread {

// }

pub struct InputManager {
    mouse_screen_pos: Arc<AtomicCell<Point>>,
    mouse_over_gui: Arc<AtomicCell<bool>>,

    winit_rx: channel::Receiver<event::WindowEvent<'static>>,

    main_view_bindings: SystemInputBindings<MainViewInputs>,
    main_view_channels: InputChannels<MainViewInputs>,

    gui_bindings: SystemInputBindings<GuiInput>,
    gui_channels: InputChannels<GuiInput>,
}

impl InputManager {
    pub fn clone_main_view_rx(
        &self,
    ) -> channel::Receiver<SystemInput<MainViewInputs>> {
        self.main_view_channels.rx.clone()
    }

    pub fn clone_gui_rx(&self) -> channel::Receiver<SystemInput<GuiInput>> {
        self.gui_channels.rx.clone()
    }

    pub fn set_mouse_over_gui(&self, is_over: bool) {
        self.mouse_over_gui.store(is_over);
    }

    pub fn handle_events(&self) {
        while let Ok(winit_ev) = self.winit_rx.try_recv() {
            if let event::WindowEvent::CursorMoved { position, .. } = winit_ev {
                self.mouse_screen_pos.store(Point {
                    x: position.x as f32,
                    y: position.y as f32,
                });
            }

            let mouse_pos = self.mouse_screen_pos.load();

            if let Some(gui_inputs) =
                self.gui_bindings.apply(&winit_ev, mouse_pos)
            {
                for input in gui_inputs {
                    self.gui_channels.tx.send(input).unwrap();
                }
            }

            if let Some(main_view_inputs) =
                self.main_view_bindings.apply(&winit_ev, mouse_pos)
            {
                let mouse_over_gui = self.mouse_over_gui.load();
                for input in main_view_inputs {
                    if input.is_keyboard()
                        || (input.is_mouse() && !mouse_over_gui)
                    {
                        self.main_view_channels.tx.send(input).unwrap();
                    }
                }
            }
        }
    }

    pub fn new(
        winit_rx: channel::Receiver<event::WindowEvent<'static>>,
    ) -> Self {
        let mouse_screen_pos = Arc::new(AtomicCell::new(Point::ZERO));
        let mouse_over_gui = Arc::new(AtomicCell::new(false));

        let main_view_bindings: SystemInputBindings<MainViewInputs> =
            Default::default();

        let gui_bindings: SystemInputBindings<GuiInput> = Default::default();

        let (main_view_tx, main_view_rx) =
            channel::unbounded::<SystemInput<MainViewInputs>>();
        let (gui_tx, gui_rx) = channel::unbounded::<SystemInput<GuiInput>>();

        let main_view_channels = InputChannels {
            tx: main_view_tx.clone(),
            rx: main_view_rx.clone(),
        };

        let gui_channels = InputChannels {
            tx: gui_tx.clone(),
            rx: gui_rx.clone(),
        };

        Self {
            mouse_screen_pos,
            mouse_over_gui,
            winit_rx,
            main_view_bindings,
            main_view_channels,
            gui_bindings,
            gui_channels,
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
    wheel_bind: WheelBind<Inputs>,
}

impl<Inputs: InputPayload> SystemInputBindings<Inputs> {
    pub fn apply(
        &self,
        // input_state: &mut InputState<Inputs>,
        event: &event::WindowEvent,
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

impl std::default::Default for SystemInputBindings<GuiInput> {
    fn default() -> Self {
        use event::VirtualKeyCode as Key;
        use GuiInput as Input;

        let key_binds: FxHashMap<event::VirtualKeyCode, Vec<KeyBind<Input>>> =
            [
                (Key::Escape, Input::KeyClearSelection),
                (Key::F1, Input::KeyEguiInspectionUi),
                (Key::F2, Input::KeyEguiSettingsUi),
                (Key::F3, Input::KeyEguiMemoryUi),
            ]
            .iter()
            .copied()
            .map(|(k, i)| (k, vec![KeyBind { payload: i }]))
            .collect::<FxHashMap<_, _>>();

        let mouse_binds: FxHashMap<
            event::MouseButton,
            Vec<MouseButtonBind<Input>>,
        > = [
            (
                event::MouseButton::Left,
                vec![MouseButtonBind {
                    payload: Input::ButtonLeft,
                }],
            ),
            (
                event::MouseButton::Right,
                vec![MouseButtonBind {
                    payload: Input::ButtonRight,
                }],
            ),
        ]
        .iter()
        .cloned()
        .collect();

        let wheel_bind = WheelBind {
            invert: false,
            mult: 1.0,
            payload: Input::WheelScroll,
        };

        Self {
            key_binds,
            mouse_binds,
            wheel_bind,
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
