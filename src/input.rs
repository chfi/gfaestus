use rustc_hash::FxHashMap;
use winit::event::{ElementState, VirtualKeyCode};
#[allow(unused_imports)]
use winit::{
    event::{self, Event, KeyboardInput, WindowEvent},
    event_loop::ControlFlow,
};

use crossbeam::atomic::AtomicCell;
use crossbeam::channel;
use std::sync::Arc;

use crate::gui::GuiInput;
use crate::{app::mainview::MainViewInput, gui::GuiMsg};
use crate::{app::AppInput, reactor::Reactor};
use crate::{app::SharedState, geometry::*};

pub mod binds;

pub use binds::{BindableInput, DigitalState, SystemInputBindings};

use binds::*;

struct SubsystemInput<T: InputPayload + BindableInput> {
    bindings: SystemInputBindings<T>,

    tx: channel::Sender<SystemInput<T>>,
    rx: channel::Receiver<SystemInput<T>>,
}

impl<T: InputPayload + BindableInput> SubsystemInput<T> {
    fn from_default_binds() -> Self {
        let bindings = T::default_binds();

        let (tx, rx) = channel::unbounded::<SystemInput<T>>();

        Self { bindings, tx, rx }
    }

    pub fn clone_rx(&self) -> channel::Receiver<SystemInput<T>> {
        self.rx.clone()
    }
}

pub struct InputManager {
    mouse_screen_pos: Arc<AtomicCell<Point>>,

    modifiers: AtomicCell<event::ModifiersState>,

    winit_rx: channel::Receiver<event::WindowEvent<'static>>,

    app: SubsystemInput<AppInput>,
    main_view: SubsystemInput<MainViewInput>,
    gui: SubsystemInput<GuiInput>,

    gui_focus_state: crate::gui::GuiFocusState,

    custom_binds: FxHashMap<
        winit::event::VirtualKeyCode,
        Arc<dyn Fn() + Send + Sync + 'static>,
        // Box<dyn Fn() + Send + Sync + 'static>,
    >,
    // custom_binds: Arc<Mutex<FxHashMap<
    //     winit::event::VirtualKeyCode,
    //     Box<dyn Fn() + Send + Sync + 'static>,
    // >>>,
}

impl InputManager {
    pub fn clone_app_rx(&self) -> channel::Receiver<SystemInput<AppInput>> {
        self.app.clone_rx()
    }

    pub fn clone_main_view_rx(
        &self,
    ) -> channel::Receiver<SystemInput<MainViewInput>> {
        self.main_view.clone_rx()
    }

    pub fn clone_gui_rx(&self) -> channel::Receiver<SystemInput<GuiInput>> {
        self.gui.clone_rx()
    }

    pub fn read_mouse_pos(&self) -> Point {
        self.mouse_screen_pos.load()
    }

    pub fn clone_mouse_pos(&self) -> Arc<AtomicCell<Point>> {
        self.mouse_screen_pos.clone()
    }

    pub fn add_binding<F>(
        &mut self,
        key_code: winit::event::VirtualKeyCode,
        command: F,
    ) where
        F: Fn() + Send + Sync + 'static,
    {
        let boxed = Arc::new(command) as Arc<dyn Fn() + Send + Sync + 'static>;
        // log::warn!("calling boxed binding command");
        // boxed();

        self.custom_binds.insert(key_code, boxed);
    }

    pub fn handle_events(
        &self,
        reactor: &mut Reactor,
        gui_msg_tx: &channel::Sender<GuiMsg>,
    ) {
        while let Ok(winit_ev) = self.winit_rx.try_recv() {
            if let event::WindowEvent::CursorMoved { position, .. } = winit_ev {
                self.mouse_screen_pos.store(Point {
                    x: position.x as f32,
                    y: position.y as f32,
                });
            }

            if let event::WindowEvent::ModifiersChanged(mods) = winit_ev {
                self.modifiers.store(mods);
                gui_msg_tx.send(GuiMsg::SetModifiers(mods)).unwrap();
            }

            let mouse_pos = self.mouse_screen_pos.load();

            let gui_wants_keyboard =
                self.gui_focus_state.wants_keyboard_input();
            let mouse_over_gui = self.gui_focus_state.mouse_over_gui();

            // NB: on my machine at least, after a file is dropped,
            // keyboard events appear to not be generated until the
            // window loses and regains focus; i'm guessing it's a
            // winit bug, or a winit + sway (+ xwayland) bug
            if let event::WindowEvent::DroppedFile(ref path) = winit_ev {
                gui_msg_tx
                    .send(GuiMsg::FileDropped { path: path.clone() })
                    .unwrap();
            }

            let modifiers = self.modifiers.load();

            if gui_wants_keyboard {
                if let event::WindowEvent::KeyboardInput { input, .. } =
                    winit_ev
                {
                    if let Some(gui_msg) =
                        input.virtual_keycode.and_then(|key| {
                            winit_to_clipboard_event(
                                modifiers,
                                input.state,
                                key,
                            )
                        })
                    {
                        gui_msg_tx.send(gui_msg).unwrap();
                    }

                    if let Some(event) = input.virtual_keycode.and_then(|key| {
                        winit_to_egui_text_event(modifiers, input.state, key)
                    }) {
                        gui_msg_tx
                            .send(crate::gui::GuiMsg::EguiEvent(event))
                            .unwrap();
                    }
                }

                if let event::WindowEvent::ReceivedCharacter(c) = winit_ev {
                    if !c.is_ascii_control() {
                        let event = received_char_to_egui_text(c);
                        gui_msg_tx
                            .send(crate::gui::GuiMsg::EguiEvent(event))
                            .unwrap();
                    }
                }
            }

            if let Some(app_inputs) =
                self.app.bindings.apply(&winit_ev, modifiers, mouse_pos)
            {
                for input in app_inputs {
                    if !(input.is_keyboard() && gui_wants_keyboard) {
                        self.app.tx.send(input).unwrap();
                    }
                }
            }

            if let Some(gui_inputs) =
                self.gui.bindings.apply(&winit_ev, modifiers, mouse_pos)
            {
                for input in gui_inputs {
                    self.gui.tx.send(input).unwrap();
                }
            }

            if let Some(main_view_inputs) = self
                .main_view
                .bindings
                .apply(&winit_ev, modifiers, mouse_pos)
            {
                for input in main_view_inputs {
                    if (input.is_keyboard() && !gui_wants_keyboard)
                        || (input.is_mouse() && !mouse_over_gui)
                        || input.is_mouse_up()
                    {
                        self.main_view.tx.send(input).unwrap();
                    }
                }
            }

            if let event::WindowEvent::KeyboardInput { input, .. } = winit_ev {
                let pressed = input.state == ElementState::Pressed;
                if pressed && !gui_wants_keyboard {
                    if let Some(command) = input
                        .virtual_keycode
                        .and_then(|kc| self.custom_binds.get(&kc))
                    {
                        log::warn!("executing bound command!");

                        let command = command.clone();

                        if let Ok(handle) =
                            reactor.spawn(async move { command() })
                        {
                            handle.forget();
                        }
                    }
                }
            }
        }
    }

    pub fn new(
        winit_rx: channel::Receiver<event::WindowEvent<'static>>,
        shared_state: &SharedState,
    ) -> Self {
        let mouse_screen_pos = shared_state.mouse_pos.clone();

        let gui_focus_state = shared_state.gui_focus_state.clone();

        let app = SubsystemInput::<AppInput>::from_default_binds();
        let main_view = SubsystemInput::<MainViewInput>::from_default_binds();
        let gui = SubsystemInput::<GuiInput>::from_default_binds();

        Self {
            mouse_screen_pos,
            winit_rx,

            modifiers: AtomicCell::new(Default::default()),

            app,
            main_view,
            gui,

            gui_focus_state,

            custom_binds: FxHashMap::default(),
        }
    }
}

fn received_char_to_egui_text(c: char) -> egui::Event {
    egui::Event::Text(c.into())
}

fn winit_to_clipboard_event(
    modifiers: event::ModifiersState,
    state: winit::event::ElementState,
    key_code: winit::event::VirtualKeyCode,
) -> Option<GuiMsg> {
    let modifiers = egui::Modifiers {
        alt: modifiers.alt(),
        ctrl: modifiers.ctrl(),
        shift: modifiers.shift(),
        mac_cmd: modifiers.logo(),
        command: modifiers.logo(),
    };

    let pressed = matches!(state, winit::event::ElementState::Pressed);

    if (modifiers.ctrl || modifiers.mac_cmd) && pressed {
        match key_code {
            VirtualKeyCode::X => Some(GuiMsg::Cut),
            VirtualKeyCode::C => Some(GuiMsg::Copy),
            VirtualKeyCode::V => Some(GuiMsg::Paste),
            _ => None,
        }
    } else {
        None
    }
}

fn winit_to_egui_text_event(
    modifiers: event::ModifiersState,
    state: winit::event::ElementState,
    key_code: winit::event::VirtualKeyCode,
) -> Option<egui::Event> {
    let modifiers = egui::Modifiers {
        alt: modifiers.alt(),
        ctrl: modifiers.ctrl(),
        shift: modifiers.shift(),
        mac_cmd: modifiers.logo(),
        command: modifiers.logo(),
    };

    let pressed = matches!(state, winit::event::ElementState::Pressed);

    let key_event = |key: egui::Key| -> Option<egui::Event> {
        Some(egui::Event::Key {
            key,
            pressed,
            modifiers,
        })
    };

    use egui::Key;

    let egui_event = match key_code {
        VirtualKeyCode::Escape => key_event(Key::Escape),
        VirtualKeyCode::Insert => key_event(Key::Insert),
        VirtualKeyCode::Home => key_event(Key::Home),
        VirtualKeyCode::Delete => key_event(Key::Delete),
        VirtualKeyCode::End => key_event(Key::End),
        VirtualKeyCode::PageDown => key_event(Key::PageDown),
        VirtualKeyCode::PageUp => key_event(Key::PageUp),
        VirtualKeyCode::Left => key_event(Key::ArrowLeft),
        VirtualKeyCode::Up => key_event(Key::ArrowUp),
        VirtualKeyCode::Right => key_event(Key::ArrowRight),
        VirtualKeyCode::Down => key_event(Key::ArrowDown),
        VirtualKeyCode::Back => key_event(Key::Backspace),
        VirtualKeyCode::Return => key_event(Key::Enter),
        VirtualKeyCode::Tab => key_event(Key::Tab),
        /*
        VirtualKeyCode::Key1 => key_event(Key::Num1),
        VirtualKeyCode::Key2 => key_event(Key::Num2),
        VirtualKeyCode::Key3 => key_event(Key::Num3),
        VirtualKeyCode::Key4 => key_event(Key::Num4),
        VirtualKeyCode::Key5 => key_event(Key::Num5),
        VirtualKeyCode::Key6 => key_event(Key::Num6),
        VirtualKeyCode::Key7 => key_event(Key::Num7),
        VirtualKeyCode::Key8 => key_event(Key::Num8),
        VirtualKeyCode::Key9 => key_event(Key::Num9),
        VirtualKeyCode::Key0 => key_event(Key::Num0),
        VirtualKeyCode::A => key_event(Key::A),
        VirtualKeyCode::B => key_event(Key::B),
        VirtualKeyCode::D => key_event(Key::D),
        VirtualKeyCode::E => key_event(Key::E),
        VirtualKeyCode::F => key_event(Key::F),
        VirtualKeyCode::G => key_event(Key::G),
        VirtualKeyCode::H => key_event(Key::H),
        VirtualKeyCode::I => key_event(Key::I),
        VirtualKeyCode::J => key_event(Key::J),
        VirtualKeyCode::K => key_event(Key::K),
        VirtualKeyCode::L => key_event(Key::L),
        VirtualKeyCode::M => key_event(Key::M),
        VirtualKeyCode::N => key_event(Key::N),
        VirtualKeyCode::O => key_event(Key::O),
        VirtualKeyCode::P => key_event(Key::P),
        VirtualKeyCode::Q => key_event(Key::Q),
        VirtualKeyCode::R => key_event(Key::R),
        VirtualKeyCode::S => key_event(Key::S),
        VirtualKeyCode::T => key_event(Key::T),
        VirtualKeyCode::U => key_event(Key::U),
        VirtualKeyCode::W => key_event(Key::W),
        VirtualKeyCode::X => key_event(Key::X),
        VirtualKeyCode::Y => key_event(Key::Y),
        VirtualKeyCode::Z => key_event(Key::Z),
        VirtualKeyCode::Space => key_event(Key::Space),
        VirtualKeyCode::Numpad0 => key_event(Key::Num0),
        VirtualKeyCode::Numpad1 => key_event(Key::Num1),
        VirtualKeyCode::Numpad2 => key_event(Key::Num2),
        VirtualKeyCode::Numpad3 => key_event(Key::Num3),
        VirtualKeyCode::Numpad4 => key_event(Key::Num4),
        VirtualKeyCode::Numpad5 => key_event(Key::Num5),
        VirtualKeyCode::Numpad6 => key_event(Key::Num6),
        VirtualKeyCode::Numpad7 => key_event(Key::Num7),
        VirtualKeyCode::Numpad8 => key_event(Key::Num8),
        VirtualKeyCode::Numpad9 => key_event(Key::Num9),
        VirtualKeyCode::NumpadAdd => text_event('+'),
        VirtualKeyCode::NumpadDivide => text_event('/'),
        VirtualKeyCode::NumpadDecimal => text_event('.'),
        VirtualKeyCode::NumpadComma => text_event(','),
        VirtualKeyCode::NumpadEnter => key_event(Key::Enter),
        VirtualKeyCode::NumpadEquals => text_event('='),
        VirtualKeyCode::NumpadMultiply => text_event('*'),
        VirtualKeyCode::NumpadSubtract => text_event('-'),
        VirtualKeyCode::Apostrophe => text_event('\''),
        VirtualKeyCode::Asterisk => text_event('*'),
        VirtualKeyCode::At => text_event('@'),
        // VirtualKeyCode::Ax => {}
        VirtualKeyCode::Backslash => text_event('\\'),
        VirtualKeyCode::Colon => text_event(':'),
        VirtualKeyCode::Comma => text_event(','),
        VirtualKeyCode::Equals => text_event('='),
        // VirtualKeyCode::Grave => {}
        VirtualKeyCode::Minus => text_event('-'),
        VirtualKeyCode::Period => text_event('.'),
        VirtualKeyCode::Plus => text_event('+'),
        VirtualKeyCode::Semicolon => text_event(';'),
        VirtualKeyCode::Slash => text_event('/'),
        VirtualKeyCode::Underline => text_event('_'),
        */
        _ => None,
    };

    egui_event
}
