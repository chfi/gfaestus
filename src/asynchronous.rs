use std::sync::Arc;

use crossbeam::atomic::AtomicCell;
use futures::{
    executor::ThreadPool,
    future::{Future, RemoteHandle},
    task::SpawnExt,
};

pub struct AsyncResult<T: Send> {
    future: Option<RemoteHandle<T>>,
    result: Option<T>,
    ready: Arc<AtomicCell<bool>>,
}

impl<T: Send> AsyncResult<T> {
    pub fn new<Fut>(thread_pool: &ThreadPool, future: Fut) -> Self
    where
        Fut: Future<Output = T> + Send + 'static,
    {
        let is_ready = Arc::new(AtomicCell::new(false));
        let inner_is_ready = is_ready.clone();

        let future = async move {
            let output = future.await;
            inner_is_ready.store(true);
            output
        };

        let handle = thread_pool.spawn_with_handle(future).unwrap();

        Self {
            future: Some(handle),
            result: None,

            ready: is_ready,
        }
    }

    pub fn is_ready(&self) -> bool {
        self.ready.load()
    }
}

impl<T: Send + 'static> AsyncResult<T> {
    pub fn get_result_if_ready(&mut self) -> Option<&T> {
        if !self.is_ready() {
            return None;
        }

        if self.result.is_some() {
            return self.result.as_ref();
        }

        if let Some(future) = self.future.take() {
            let value = futures::executor::block_on(future);
            self.result = Some(value);
        }

        self.result.as_ref()
    }

    pub fn move_result_if_ready(&mut self) {
        if !self.is_ready() || self.result.is_some() {
            return;
        }

        if let Some(future) = self.future.take() {
            let value = futures::executor::block_on(future);
            self.result = Some(value);
        }
    }

    pub fn get_result(&self) -> Option<&T> {
        self.result.as_ref()
    }

    pub fn take_result_if_ready(&mut self) -> Option<T> {
        if !self.is_ready() {
            return None;
        }

        self.move_result_if_ready();

        self.result.take()
    }
}
