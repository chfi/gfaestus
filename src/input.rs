#[allow(unused_imports)]
use winit::{
    event::{self, Event, KeyboardInput, WindowEvent},
    event_loop::ControlFlow,
};

use crossbeam::atomic::AtomicCell;
use crossbeam::channel;
use std::sync::Arc;

use crate::geometry::*;

pub mod binds;

pub use binds::DigitalState;

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

struct InputChannels<T: InputPayload> {
    tx: channel::Sender<SystemInput<T>>,
    rx: channel::Receiver<SystemInput<T>>,
}

pub struct InputManager {
    mouse_screen_pos: MousePos,
    mouse_over_gui: Arc<AtomicCell<bool>>,

    winit_rx: channel::Receiver<event::WindowEvent<'static>>,

    app_bindings: SystemInputBindings<AppInput>,
    app_channels: InputChannels<AppInput>,

    main_view_bindings: SystemInputBindings<MainViewInputs>,
    main_view_channels: InputChannels<MainViewInputs>,

    gui_bindings: SystemInputBindings<GuiInput>,
    gui_channels: InputChannels<GuiInput>,
}

impl InputManager {
    pub fn clone_app_rx(&self) -> channel::Receiver<SystemInput<AppInput>> {
        self.app_channels.rx.clone()
    }

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

            if let Some(app_inputs) =
                self.app_bindings.apply(&winit_ev, mouse_pos)
            {
                for input in app_inputs {
                    self.app_channels.tx.send(input).unwrap();
                }
            }

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
        let mouse_screen_pos = MousePos::new(Point::ZERO);
        let mouse_over_gui = Arc::new(AtomicCell::new(false));

        let app_bindings: SystemInputBindings<AppInput> = Default::default();

        let main_view_bindings: SystemInputBindings<MainViewInputs> =
            Default::default();

        let gui_bindings: SystemInputBindings<GuiInput> = Default::default();

        let (app_tx, app_rx) = channel::unbounded::<SystemInput<AppInput>>();

        let (main_view_tx, main_view_rx) =
            channel::unbounded::<SystemInput<MainViewInputs>>();

        let (gui_tx, gui_rx) = channel::unbounded::<SystemInput<GuiInput>>();

        let app_channels = InputChannels {
            tx: app_tx.clone(),
            rx: app_rx.clone(),
        };

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
            app_bindings,
            app_channels,
            main_view_bindings,
            main_view_channels,
            gui_bindings,
            gui_channels,
        }
    }
}
