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
