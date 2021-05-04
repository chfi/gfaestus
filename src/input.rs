#[allow(unused_imports)]
use winit::{
    event::{self, Event, KeyboardInput, WindowEvent},
    event_loop::ControlFlow,
};

use crossbeam::atomic::AtomicCell;
use crossbeam::channel;
use std::sync::Arc;

use crate::app::mainview::MainViewInput;
use crate::app::AppInput;
use crate::geometry::*;
use crate::gui::GuiInput;

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
}

impl InputManager {
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

    pub fn handle_events(&self) {
        while let Ok(winit_ev) = self.winit_rx.try_recv() {
            if let event::WindowEvent::CursorMoved { position, .. } = winit_ev {
                self.mouse_screen_pos.store(Point {
                    x: position.x as f32,
                    y: position.y as f32,
                });
            }

            let mouse_pos = self.mouse_screen_pos.read();

            if let Some(app_inputs) = self.app.bindings.apply(&winit_ev, mouse_pos) {
                for input in app_inputs {
                    self.app.tx.send(input).unwrap();
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
                    if input.is_keyboard()
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
        }
    }
}
