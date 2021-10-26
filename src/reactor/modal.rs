use crossbeam::atomic::AtomicCell;
use futures::{future::RemoteHandle, SinkExt};
use parking_lot::{
    Mutex, RwLock, RwLockReadGuard, RwLockUpgradableReadGuard, RwLockWriteGuard,
};
use std::sync::Arc;

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
    active_modal: Option<Box<dyn Fn(&mut egui::Ui) + Send + Sync + 'static>>,

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
        F: Fn(&mut T, &mut egui::Ui) -> Result<ModalSuccess, ModalError>
            + Send
            + Sync
            + 'static,
        T: std::fmt::Debug + Clone + Send + Sync + 'static,
    {
        let store = Arc::new(Mutex::new(value));

        let show_modal = show_modal.clone();

        let callback = move |val: &mut T, ui: &mut egui::Ui| {
            let inner_result = callback(val, ui);

            if matches!(
                inner_result,
                Ok(ModalSuccess::Success | ModalSuccess::Cancel)
            ) {
                return inner_result;
            }

            let (accept, cancel) = ui
                .horizontal(|ui| {
                    let accept = ui.button("Accept");
                    let cancel = ui.button("Cancel");
                    (accept, cancel)
                })
                .inner;

            if accept.clicked() {
                return Ok(ModalSuccess::Success);
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
        if self.active_modal.is_some() {
            anyhow::bail!("Tried adding a modal when one was already active")
        }

        self.active_modal = Some(callback);
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
        if self.active_modal.is_some() {
            anyhow::bail!("Tried adding a modal when one was already active")
        }
        // let store = store.to_owned();

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

        self.active_modal = Some(wrapped);

        self.show_modal.store(true);

        Ok(res_rx)
    }

    pub fn show(&mut self, ctx: &egui::CtxRef) {
        if let Some(wrapped) = &self.active_modal {
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
        if !self.show_modal.load() && self.active_modal.is_some() {
            self.active_modal.take();
        }
    }
}

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
