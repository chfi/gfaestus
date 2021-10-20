use futures::{future::RemoteHandle, SinkExt};
use parking_lot::{
    Mutex, RwLock, RwLockReadGuard, RwLockUpgradableReadGuard, RwLockWriteGuard,
};
use std::sync::Arc;

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
    Error(String),
}

#[derive(Default)]
pub struct ModalHandler {
    // active_modal: Option<Box<dyn FnMut(&mut egui::Ui) + Send + Sync + 'static>>,
    active_modal: Option<Box<dyn Fn(&mut egui::Ui) + Send + Sync + 'static>>,
    // active_modal: Option<
    //     Box<
    //         dyn FnMut(&mut egui::Ui) -> Result<ModalSuccess, ModalError>
    //             + Send
    //             + Sync
    //             + 'static,
    //     >,
    // >,
}

impl ModalHandler {
    pub fn set_active<F, T>(
        &mut self,
        callback: F,
        store: &Arc<RwLock<T>>,
        // ) -> anyhow::Result<RemoteHandle<T>>
        // ) -> anyhow::Result<futures::channel::oneshot::Receiver<Option<T>>>
    ) -> anyhow::Result<futures::channel::mpsc::Receiver<Option<T>>>
    where
        F: Fn(&mut T, &mut egui::Ui) -> Result<ModalSuccess, ModalError>
            + Send
            + Sync
            + 'static,
        T: Clone + Send + Sync + 'static,
    {
        // let store = store.to_owned();

        let value: Arc<Mutex<T>> = {
            let lock = store.read();
            Arc::new(Mutex::new(lock.clone()))
        };

        let store = store.to_owned();

        let (res_tx, res_rx) = futures::channel::mpsc::channel::<Option<T>>(1);
        // futures::channel::oneshot::channel::<Option<T>>();

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
                    let _ = res_tx.send(Some(output));
                }
                Ok(ModalSuccess::Cancel) => {
                    // don't replace the stored value
                    // so basically don't do anything
                    let output = {
                        let lock = store.read();
                        lock.to_owned()
                    };
                    let _ = res_tx.send(Some(output));
                }
                Err(error) => {
                    // update modal UI error/feedback message state
                    let _ = res_tx.send(None);
                }
            };
            // let mut value = value;
            // let value =

            // unimplemented!();
        })
            as Box<dyn Fn(&mut egui::Ui) + Send + Sync + 'static>;
        // as Box<dyn FnOnce(&mut egui::Ui) + Send + Sync + 'static>;
        // as Box<dyn FnMut(&mut egui::Ui) + Send + Sync + 'static>;
        // as Box<dyn for<'r> FnMut(&'r mut egui::Ui) + Send + Sync + 'static>;

        self.active_modal = Some(wrapped);

        Ok(res_rx)
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
