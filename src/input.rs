use winit::event::VirtualKeyCode;
#[allow(unused_imports)]
use winit::{
    event::{self, Event, KeyboardInput, WindowEvent},
    event_loop::ControlFlow,
};

use crossbeam::atomic::AtomicCell;
use crossbeam::channel;
use std::sync::Arc;

use crate::app::AppInput;
use crate::geometry::*;
use crate::gui::GuiInput;
use crate::{app::mainview::MainViewInput, gui::GuiMsg};

pub mod binds;

pub use binds::{BindableInput, DigitalState, SystemInputBindings};

use binds::*;

/// A wrapper over `Arc<AtomicCell<Point>>`, which can be shared
/// across systems, but only the InputManager has access to the
/// contents & the mutation method
#[derive(Clone)]
pub struct MousePos {
    pos: Arc<AtomicCell<Point>>,
}

impl MousePos {
    fn new(point: Point) -> Self {
        Self {
            pos: Arc::new(AtomicCell::new(point)),
        }
    }

    fn store(&self, new: Point) {
        self.pos.store(new);
    }

    pub fn read(&self) -> Point {
        self.pos.load()
    }
}

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
    mouse_screen_pos: MousePos,
    mouse_over_gui: Arc<AtomicCell<bool>>,

    winit_rx: channel::Receiver<event::WindowEvent<'static>>,

    app: SubsystemInput<AppInput>,
    main_view: SubsystemInput<MainViewInput>,
    gui: SubsystemInput<GuiInput>,

    gui_focus_state: crate::gui::GuiFocusState,
}

impl InputManager {
    pub fn gui_focus_state(&self) -> &crate::gui::GuiFocusState {
        &self.gui_focus_state
    }

    pub fn clone_app_rx(&self) -> channel::Receiver<SystemInput<AppInput>> {
        self.app.clone_rx()
    }

    pub fn clone_main_view_rx(&self) -> channel::Receiver<SystemInput<MainViewInput>> {
        self.main_view.clone_rx()
    }

    pub fn clone_gui_rx(&self) -> channel::Receiver<SystemInput<GuiInput>> {
        self.gui.clone_rx()
    }

    pub fn set_mouse_over_gui(&self, is_over: bool) {
        self.mouse_over_gui.store(is_over);
    }

    pub fn read_mouse_pos(&self) -> Point {
        self.mouse_screen_pos.pos.load()
    }

    pub fn clone_mouse_pos(&self) -> MousePos {
        self.mouse_screen_pos.clone()
    }

    pub fn handle_events(&self, gui_msg_tx: &channel::Sender<GuiMsg>) {
        while let Ok(winit_ev) = self.winit_rx.try_recv() {
            if let event::WindowEvent::CursorMoved { position, .. } = winit_ev {
                self.mouse_screen_pos.store(Point {
                    x: position.x as f32,
                    y: position.y as f32,
                });
            }

            let mouse_pos = self.mouse_screen_pos.read();

            let gui_wants_keyboard = self.gui_focus_state.wants_keyboard_input();

            // if let event::WindowEvent::HoveredFile(ref path) = winit_ev {
            //     println!("file hover");
            // }

            if let event::WindowEvent::DroppedFile(ref path) = winit_ev {
                println!("file dropped");
                gui_msg_tx
                    .send(GuiMsg::FileDropped {
                        mouse_pos,
                        path: path.clone(),
                    })
                    .unwrap();
            }

            if gui_wants_keyboard {
                if let event::WindowEvent::KeyboardInput { input, .. } = winit_ev {
                    if let Some(event) = input
                        .virtual_keycode
                        .and_then(|key| winit_to_egui_text_event(input.state, key))
                    {
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

            if let Some(app_inputs) = self.app.bindings.apply(&winit_ev, mouse_pos) {
                for input in app_inputs {
                    if !(input.is_keyboard() && gui_wants_keyboard) {
                        self.app.tx.send(input).unwrap();
                    }
                }
            }

            if let Some(gui_inputs) = self.gui.bindings.apply(&winit_ev, mouse_pos) {
                for input in gui_inputs {
                    self.gui.tx.send(input).unwrap();
                }
            }

            if let Some(main_view_inputs) = self.main_view.bindings.apply(&winit_ev, mouse_pos) {
                let mouse_over_gui = self.mouse_over_gui.load();

                for input in main_view_inputs {
                    if (input.is_keyboard() && !gui_wants_keyboard)
                        || (input.is_mouse() && !mouse_over_gui)
                        || input.is_mouse_up()
                    {
                        self.main_view.tx.send(input).unwrap();
                    }
                }
            }
        }
    }

    pub fn new(winit_rx: channel::Receiver<event::WindowEvent<'static>>) -> Self {
        let mouse_screen_pos = MousePos::new(Point::ZERO);
        let mouse_over_gui = Arc::new(AtomicCell::new(false));

        let app = SubsystemInput::<AppInput>::from_default_binds();
        let main_view = SubsystemInput::<MainViewInput>::from_default_binds();
        let gui = SubsystemInput::<GuiInput>::from_default_binds();

        Self {
            mouse_screen_pos,
            mouse_over_gui,
            winit_rx,

            app,
            main_view,
            gui,

            gui_focus_state: Default::default(),
        }
    }
}

fn received_char_to_egui_text(c: char) -> egui::Event {
    egui::Event::Text(c.into())
}

fn winit_to_egui_text_event(
    state: winit::event::ElementState,
    key_code: winit::event::VirtualKeyCode,
) -> Option<egui::Event> {
    // TODO track modifiers with winit's ModifiersChanged event
    let modifiers = egui::Modifiers {
        alt: false,
        ctrl: false,
        shift: false,
        mac_cmd: false,
        command: false,
    };

    let pressed = matches!(state, winit::event::ElementState::Pressed);

    fn text_event(c: char) -> Option<egui::Event> {
        Some(egui::Event::Text(c.into()))
    }

    let key_event = |key: egui::Key| -> Option<egui::Event> {
        Some(egui::Event::Key {
            key,
            pressed,
            modifiers,
        })
    };

    use egui::Key;

    let egui_event = match key_code {
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
        VirtualKeyCode::C => key_event(Key::C),
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
        VirtualKeyCode::V => key_event(Key::V),
        VirtualKeyCode::W => key_event(Key::W),
        VirtualKeyCode::X => key_event(Key::X),
        VirtualKeyCode::Y => key_event(Key::Y),
        VirtualKeyCode::Z => key_event(Key::Z),
        */
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

/*
fn winit_to_egui_keyboard_input(
    modifiers: winit::event::ModifiersState,
    state: winit::event::ElementState,
    key_code: winit::event::VirtualKeyCode,
) -> Option<egui::Event> {

    let modifiers = egui::Modifiers {
        alt: modifiers.alt(),
        ctrl: modifiers.ctrl(),
        shift: modifiers.shift(),
        // TODO this has to be fixed according to the egui docs
        mac_cmd: modifiers.logo(),
        command: modifiers.ctrl(),
    };


    let pressed = matches!(state, winit::event::ElementState::Pressed);

    let key = match key_code {
        VirtualKeyCode::Key1 => {}
        VirtualKeyCode::Key2 => {}
        VirtualKeyCode::Key3 => {}
        VirtualKeyCode::Key4 => {}
        VirtualKeyCode::Key5 => {}
        VirtualKeyCode::Key6 => {}
        VirtualKeyCode::Key7 => {}
        VirtualKeyCode::Key8 => {}
        VirtualKeyCode::Key9 => {}
        VirtualKeyCode::Key0 => {}
        VirtualKeyCode::A => {}
        VirtualKeyCode::B => {}
        VirtualKeyCode::C => {}
        VirtualKeyCode::D => {}
        VirtualKeyCode::E => {}
        VirtualKeyCode::F => {}
        VirtualKeyCode::G => {}
        VirtualKeyCode::H => {}
        VirtualKeyCode::I => {}
        VirtualKeyCode::J => {}
        VirtualKeyCode::K => {}
        VirtualKeyCode::L => {}
        VirtualKeyCode::M => {}
        VirtualKeyCode::N => {}
        VirtualKeyCode::O => {}
        VirtualKeyCode::P => {}
        VirtualKeyCode::Q => {}
        VirtualKeyCode::R => {}
        VirtualKeyCode::S => {}
        VirtualKeyCode::T => {}
        VirtualKeyCode::U => {}
        VirtualKeyCode::V => {}
        VirtualKeyCode::W => {}
        VirtualKeyCode::X => {}
        VirtualKeyCode::Y => {}
        VirtualKeyCode::Z => {}
        VirtualKeyCode::Escape => {}
        VirtualKeyCode::Insert => {}
        VirtualKeyCode::Home => {}
        VirtualKeyCode::Delete => {}
        VirtualKeyCode::End => {}
        VirtualKeyCode::PageDown => {}
        VirtualKeyCode::PageUp => {}
        VirtualKeyCode::Left => {}
        VirtualKeyCode::Up => {}
        VirtualKeyCode::Right => {}
        VirtualKeyCode::Down => {}
        VirtualKeyCode::Back => {}
        VirtualKeyCode::Return => {}
        VirtualKeyCode::Space => {}
        VirtualKeyCode::Numpad0 => {}
        VirtualKeyCode::Numpad1 => {}
        VirtualKeyCode::Numpad2 => {}
        VirtualKeyCode::Numpad3 => {}
        VirtualKeyCode::Numpad4 => {}
        VirtualKeyCode::Numpad5 => {}
        VirtualKeyCode::Numpad6 => {}
        VirtualKeyCode::Numpad7 => {}
        VirtualKeyCode::Numpad8 => {}
        VirtualKeyCode::Numpad9 => {}
        VirtualKeyCode::NumpadAdd => {}
        VirtualKeyCode::NumpadDivide => {}
        VirtualKeyCode::NumpadDecimal => {}
        VirtualKeyCode::NumpadComma => {}
        VirtualKeyCode::NumpadEnter => {}
        VirtualKeyCode::NumpadEquals => {}
        VirtualKeyCode::NumpadMultiply => {}
        VirtualKeyCode::NumpadSubtract => {}
        VirtualKeyCode::AbntC1 => {}
        VirtualKeyCode::AbntC2 => {}
        VirtualKeyCode::Apostrophe => {}
        VirtualKeyCode::Apps => {}
        VirtualKeyCode::Asterisk => {}
        VirtualKeyCode::At => {}
        VirtualKeyCode::Ax => {}
        VirtualKeyCode::Backslash => {}
        VirtualKeyCode::Calculator => {}
        VirtualKeyCode::Capital => {}
        VirtualKeyCode::Colon => {}
        VirtualKeyCode::Comma => {}
        VirtualKeyCode::Convert => {}
        VirtualKeyCode::Equals => {}
        VirtualKeyCode::Grave => {}
        VirtualKeyCode::Kana => {}
        VirtualKeyCode::Kanji => {}
        VirtualKeyCode::LAlt => {}
        VirtualKeyCode::LBracket => {}
        VirtualKeyCode::LControl => {}
        VirtualKeyCode::LShift => {}
        VirtualKeyCode::LWin => {}
        VirtualKeyCode::Mail => {}
        VirtualKeyCode::MediaSelect => {}
        VirtualKeyCode::MediaStop => {}
        VirtualKeyCode::Minus => {}
        VirtualKeyCode::Mute => {}
        VirtualKeyCode::MyComputer => {}
        VirtualKeyCode::NavigateForward => {}
        VirtualKeyCode::NavigateBackward => {}
        VirtualKeyCode::NextTrack => {}
        VirtualKeyCode::NoConvert => {}
        VirtualKeyCode::OEM102 => {}
        VirtualKeyCode::Period => {}
        VirtualKeyCode::PlayPause => {}
        VirtualKeyCode::Plus => {}
        VirtualKeyCode::Power => {}
        VirtualKeyCode::PrevTrack => {}
        VirtualKeyCode::RAlt => {}
        VirtualKeyCode::RBracket => {}
        VirtualKeyCode::RControl => {}
        VirtualKeyCode::RShift => {}
        VirtualKeyCode::RWin => {}
        VirtualKeyCode::Semicolon => {}
        VirtualKeyCode::Slash => {}
        VirtualKeyCode::Sleep => {}
        VirtualKeyCode::Stop => {}
        VirtualKeyCode::Sysrq => {}
        VirtualKeyCode::Tab => {}
        VirtualKeyCode::Underline => {}
        VirtualKeyCode::Unlabeled => {}
        VirtualKeyCode::VolumeDown => {}
        VirtualKeyCode::VolumeUp => {}
        VirtualKeyCode::Wake => {}
        VirtualKeyCode::WebBack => {}
        VirtualKeyCode::WebFavorites => {}
        VirtualKeyCode::WebForward => {}
        VirtualKeyCode::WebHome => {}
        VirtualKeyCode::WebRefresh => {}
        VirtualKeyCode::WebSearch => {}
        VirtualKeyCode::WebStop => {}
        VirtualKeyCode::Yen => {}
        VirtualKeyCode::Copy => {}
        VirtualKeyCode::Paste => {}
        VirtualKeyCode::Cut => {}
    };

}
*/
