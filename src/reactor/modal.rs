use crossbeam::atomic::AtomicCell;
use futures::{future::RemoteHandle, Future, SinkExt, StreamExt};
use parking_lot::{
    Mutex, RwLock, RwLockReadGuard, RwLockUpgradableReadGuard, RwLockWriteGuard,
};
use std::{collections::VecDeque, path::PathBuf, sync::Arc};

use crate::geometry::Point;

pub trait CallbackTrait<T>:
    Fn(&mut T, &mut egui::Ui) -> anyhow::Result<()> + Send + Sync + 'static
{
}

impl<T, U> CallbackTrait<T> for U where
    U: Fn(&mut T, &mut egui::Ui) -> anyhow::Result<()> + Send + Sync + 'static
{
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModalSuccess {
    Success,
    Cancel,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ModalError {
    Continue,
    Error(String),
}

#[derive(Default)]
pub struct ModalHandler {
    modal_stack: VecDeque<Box<dyn Fn(&mut egui::Ui) + Send + Sync + 'static>>,
    pub show_modal: Arc<AtomicCell<bool>>,
}

impl ModalHandler {
    pub fn new(show_modal: Arc<AtomicCell<bool>>) -> Self {
        Self {
            show_modal,
            ..Self::default()
        }
    }

    pub fn get_string(
        &mut self,
    ) -> anyhow::Result<futures::channel::mpsc::Receiver<Option<String>>> {
        let store = Arc::new(RwLock::new(String::new()));

        self.set_active(&store, |text, ui| {
            let _text_box = ui.text_edit_singleline(text);

            let resp = ui.horizontal(|ui| {
                let ok_btn = ui.button("OK");
                let cancel_btn = ui.button("cancel");

                (ok_btn, cancel_btn)
            });

            let (ok_btn, cancel_btn) = resp.inner;

            if ok_btn.clicked() {
                return Ok(ModalSuccess::Success);
            }

            if cancel_btn.clicked() {
                return Ok(ModalSuccess::Cancel);
            }

            Err(ModalError::Continue)
        })
    }

    pub fn prepare_callback<F, T>(
        // &self,
        show_modal: &Arc<AtomicCell<bool>>,
        value: T,
        callback: F,
        res_tx: futures::channel::mpsc::Sender<Option<T>>,
    ) -> Box<dyn Fn(&mut egui::Ui) + Send + Sync + 'static>
    where
        F: Fn(&mut T, &mut egui::Ui, bool) -> Result<ModalSuccess, ModalError>
            + Send
            + Sync
            + 'static,
        T: std::fmt::Debug + Clone + Send + Sync + 'static,
    {
        let store = Arc::new(Mutex::new(value));

        let show_modal = show_modal.clone();

        let callback = move |val: &mut T, ui: &mut egui::Ui| {
            let (accept, cancel) = ui
                .horizontal(|ui| {
                    let accept = ui.button("Accept");
                    let cancel = ui.button("Cancel");
                    (accept, cancel)
                })
                .inner;

            let inner_result = callback(val, ui, accept.clicked());

            if matches!(
                inner_result,
                Ok(ModalSuccess::Success | ModalSuccess::Cancel)
            ) {
                return inner_result;
            }

            if cancel.clicked() {
                return Ok(ModalSuccess::Cancel);
            }

            Err(ModalError::Continue)
        };

        let wrapped = Box::new(move |ui: &mut egui::Ui| {
            let mut res_tx = res_tx.clone();
            let result = {
                let mut lock = store.lock();
                let result = callback(&mut lock, ui);
                result
            };

            match result {
                Ok(ModalSuccess::Success) => {
                    // replace the stored value
                    let output = {
                        let lock = store.lock();
                        lock.to_owned()
                    };
                    let _ = res_tx.try_send(Some(output));

                    show_modal.store(false);
                }
                Ok(ModalSuccess::Cancel) => {
                    // don't replace the stored value
                    // so basically don't do anything
                    let output = {
                        let lock = store.lock();
                        lock.to_owned()
                    };
                    let _ = res_tx.try_send(Some(output));
                    show_modal.store(false);
                }
                Err(ModalError::Continue) => {
                    // don't do anything in this case
                }
                Err(error) => {
                    // update modal UI error/feedback message state
                    let _ = res_tx.try_send(None);
                }
            };
        })
            as Box<dyn Fn(&mut egui::Ui) + Send + Sync + 'static>;

        wrapped
    }

    pub fn set_prepared_active(
        &mut self,
        callback: Box<dyn Fn(&mut egui::Ui) + Send + Sync + 'static>,
    ) -> anyhow::Result<()> {
        self.modal_stack.push_back(callback);
        self.show_modal.store(true);
        Ok(())
    }

    pub fn set_active<F, T>(
        &mut self,
        store: &Arc<RwLock<T>>,
        callback: F,
    ) -> anyhow::Result<futures::channel::mpsc::Receiver<Option<T>>>
    where
        F: Fn(&mut T, &mut egui::Ui) -> Result<ModalSuccess, ModalError>
            + Send
            + Sync
            + 'static,
        T: std::fmt::Debug + Clone + Send + Sync + 'static,
    {
        let value: Arc<Mutex<T>> = {
            let lock = store.read();
            Arc::new(Mutex::new(lock.clone()))
        };

        let store = store.to_owned();

        let (res_tx, res_rx) = futures::channel::mpsc::channel::<Option<T>>(1);
        // futures::channel::oneshot::channel::<Option<T>>();

        let show_modal = self.show_modal.clone();

        let wrapped = Box::new(move |ui: &mut egui::Ui| {
            // let value = value;

            let mut res_tx = res_tx.clone();
            let result = {
                let mut lock = value.lock();
                let result = callback(&mut lock, ui);
                result
            };

            match result {
                Ok(ModalSuccess::Success) => {
                    // replace the stored value
                    let output = {
                        let lock = value.lock();
                        lock.to_owned()
                    };
                    let _ = res_tx.try_send(Some(output));

                    show_modal.store(false);
                }
                Ok(ModalSuccess::Cancel) => {
                    // don't replace the stored value
                    // so basically don't do anything
                    let output = {
                        let lock = store.read();
                        lock.to_owned()
                    };
                    let _ = res_tx.try_send(Some(output));
                    show_modal.store(false);
                }
                Err(ModalError::Continue) => {
                    // don't do anything in this case
                }
                Err(error) => {
                    // update modal UI error/feedback message state
                    let _ = res_tx.try_send(None);
                }
            };
        })
            as Box<dyn Fn(&mut egui::Ui) + Send + Sync + 'static>;

        self.modal_stack.push_back(wrapped);

        self.show_modal.store(true);

        Ok(res_rx)
    }

    pub fn show(&mut self, ctx: &egui::CtxRef) {
        if let Some(wrapped) = self.modal_stack.back() {
            if self.show_modal.load() {
                egui::Window::new("Modal")
                    .id(egui::Id::new("modal_window"))
                    .anchor(egui::Align2::CENTER_CENTER, Point::ZERO)
                    // .anchor(egui::Align2::CENTER_TOP, Point::new(0.0, 50.0))
                    .title_bar(false)
                    .collapsible(false)
                    .show(ctx, |mut ui| {
                        wrapped(&mut ui);
                    });
            }
        }

        // kinda hacky but this should make sure there only is an
        // active modal when it should be rendered
        if !self.show_modal.load() {
            self.modal_stack.pop_back();
            self.show_modal.store(self.modal_stack.is_empty());
        }
    }
}

pub fn file_picker_modal(
    modal_tx: crossbeam::channel::Sender<
        Box<dyn Fn(&mut egui::Ui) + Send + Sync + 'static>,
    >,
    show_modal: &Arc<AtomicCell<bool>>,
    extensions: &[&str],
    dir: Option<PathBuf>,
) -> impl Future<Output = Option<PathBuf>> + Send + Sync + 'static {
    use crate::gui::windows::file::FilePicker;

    let pwd = dir.unwrap_or_else(|| std::fs::canonicalize("./").unwrap());

    let mut file_picker =
        FilePicker::new(egui::Id::new("_file_picker"), pwd).unwrap();
    file_picker.set_visible_extensions(extensions).unwrap();

    let closure =
        move |state: &mut FilePicker, ui: &mut egui::Ui, force: bool| {
            if let Ok(v) = state.ui_impl(ui, force) {
                state.selected_path = state.highlighted_dir.clone();
                return Ok(v);
            }
            Err(ModalError::Continue)
        };

    let (result_tx, mut result_rx) =
        futures::channel::mpsc::channel::<Option<FilePicker>>(1);

    let prepared = ModalHandler::prepare_callback(
        show_modal,
        file_picker,
        closure,
        result_tx,
    );

    modal_tx.send(prepared).unwrap();

    async move {
        let final_state = result_rx.next().await.flatten();
        final_state.and_then(|state| state.selected_path)
    }
}

/*
pub type ModalCallback<T> = Box<dyn CallbackTrait<T>>;

pub struct ModalValue<T>
where
    T: Clone + Send + Sync + 'static,
{
    store: Arc<RwLock<T>>,
    rx: crossbeam::channel::Receiver<T>,
    tx: crossbeam::channel::Sender<T>,

    callback: ModalCallback<T>,
    // callback: Box<dyn Fn(&mut egui::Ui) -> T>,
}

impl<T> ModalValue<T>
where
    T: Clone + Send + Sync + 'static,
{
    pub fn new_with<F>(callback: F, v: T) -> Self
    where
        F: Fn(&mut T, &mut egui::Ui) -> anyhow::Result<()>
            + Send
            + Sync
            + 'static,
    {
        let (tx, rx) = crossbeam::channel::unbounded::<T>();

        let store = Arc::new(RwLock::new(v));

        let callback = Box::new(callback) as ModalCallback<T>;

        Self {
            store,
            rx,
            tx,
            callback,
        }
    }

    pub fn store(&self) -> &Arc<RwLock<T>> {
        &self.store
    }

    pub fn ui_blocking(&self, ui: &mut egui::Ui) -> anyhow::Result<()> {
        let mut lock = self.store.write();
        let callback = &self.callback;
        callback(&mut lock, ui)?;
        Ok(())
    }

    pub fn read(&self) -> RwLockReadGuard<'_, T> {
        self.store.read()
    }

    pub fn try_read(&self) -> Option<RwLockReadGuard<'_, T>> {
        self.store.try_read()
    }
}

*/
