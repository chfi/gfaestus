use std::pin::Pin;
use std::sync::Arc;

use clipboard::{ClipboardContext, ClipboardProvider};
use crossbeam::channel::{Receiver, Sender};
use futures::{future::RemoteHandle, task::SpawnExt, Future};

mod modal;
mod paired;

pub use modal::*;
pub use paired::{create_host_pair, Host, Inbox, Outbox, Processor};

use paired::*;
use parking_lot::lock_api::RawMutex;
use parking_lot::Mutex;

use crate::app::channels::OverlayCreatorMsg;
use crate::app::AppChannels;
use crate::graph_query::GraphQuery;
use crate::vulkan::GpuTasks;

pub struct Reactor {
    pub thread_pool: futures::executor::ThreadPool,
    pub rayon_pool: Arc<rayon::ThreadPool>,

    pub graph_query: Arc<GraphQuery>,

    pub overlay_create_tx: Sender<OverlayCreatorMsg>,
    pub overlay_create_rx: Receiver<OverlayCreatorMsg>,

    pub gpu_tasks: Arc<GpuTasks>,

    pub clipboard_ctx: Arc<Mutex<ClipboardContext>>,

    pub future_tx:
        Sender<Pin<Box<dyn Future<Output = ()> + Send + Sync + 'static>>>,
    // pub future_tx: Sender<Box<dyn Future<Output = ()> + 'static>>,

    // pub future_tx: Sender<Box<dyn FnOnce() + Send + Sync + 'static>>,
    // pub task_rx: Receiver<Box<dyn FnOnce() + Send + Sync + 'static>>,
    _task_thread: std::thread::JoinHandle<()>,
}

impl Reactor {
    pub fn init(
        thread_pool: futures::executor::ThreadPool,
        rayon_pool: rayon::ThreadPool,
        graph_query: Arc<GraphQuery>,
        channels: &AppChannels,
    ) -> Self {
        let rayon_pool = Arc::new(rayon_pool);

        let (task_tx, task_rx) = crossbeam::channel::unbounded();

        let thread_pool_ = thread_pool.clone();

        let _task_thread = std::thread::spawn(move || {
            let thread_pool = thread_pool_;

            while let Ok(task) = task_rx.recv() {
                thread_pool.spawn(task).unwrap();
            }
        });

        let clipboard_ctx =
            Arc::new(Mutex::new(ClipboardProvider::new().unwrap()));

        Self {
            thread_pool,
            rayon_pool,

            graph_query,

            gpu_tasks: Arc::new(GpuTasks::default()),

            clipboard_ctx,

            overlay_create_tx: channels.new_overlay_tx.clone(),
            overlay_create_rx: channels.new_overlay_rx.clone(),

            future_tx: task_tx,
            // task_rx,
            _task_thread,
        }
    }

    pub fn set_clipboard_contents(&self, contents: &str, block: bool) {
        if block {
            let mut ctx = self.clipboard_ctx.lock();
            ctx.set_contents(contents.to_string()).unwrap();
        } else if let Some(mut ctx) = self.clipboard_ctx.try_lock() {
            ctx.set_contents(contents.to_string()).unwrap();
        }
    }

    pub fn get_clipboard_contents(&self, block: bool) -> Option<String> {
        if block {
            let mut ctx = self.clipboard_ctx.lock();
            ctx.get_contents().ok()
        } else if let Some(mut ctx) = self.clipboard_ctx.try_lock() {
            ctx.get_contents().ok()
        } else {
            None
        }
    }

    pub fn create_host<F, I, T>(&self, func: F) -> Host<I, T>
    where
        T: Send + Sync + 'static,
        I: Send + Sync + 'static,
        F: Fn(&Outbox<T>, I) -> T + Send + Sync + 'static,
    {
        let boxed_func = Box::new(func) as Box<_>;

        let (host, proc) = create_host_pair(boxed_func);

        let mut processor = Box::new(proc) as Box<dyn ProcTrait>;

        self.thread_pool
            .spawn(async move {
                log::debug!("spawning reactor task");

                loop {
                    let _result = processor.process().await;
                }
            })
            .expect("Error when spawning reactor task");

        host
    }

    pub fn spawn_interval<F>(
        &self,
        mut func: F,
        dur: std::time::Duration,
    ) -> anyhow::Result<RemoteHandle<()>>
    where
        F: FnMut() + Send + Sync + 'static,
    {
        use futures_timer::Delay;

        let result = self.thread_pool.spawn_with_handle(async move {
            /*
            let looper = || {
                let delay = Delay::new(dur);
                async {
                    delay.await;
                    func();
                }
            };
            */

            loop {
                let delay = Delay::new(dur);
                delay.await;
                func();
            }
        })?;
        Ok(result)
    }

    pub fn spawn<F, T>(&self, fut: F) -> anyhow::Result<RemoteHandle<T>>
    where
        F: Future<Output = T> + Send + Sync + 'static,
        T: Send + Sync + 'static,
    {
        let handle = self.thread_pool.spawn_with_handle(fut)?;
        Ok(handle)
    }

    pub fn spawn_forget<F>(&self, fut: F) -> anyhow::Result<()>
    where
        F: Future<Output = ()> + Send + Sync + 'static,
    {
        let fut = Box::pin(fut) as _;
        self.future_tx.send(fut)?;
        Ok(())
    }
}
