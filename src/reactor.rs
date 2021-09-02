use std::sync::Arc;

use crossbeam::channel::{Receiver, Sender};
use futures::{future::RemoteHandle, task::SpawnExt, Future};

mod paired;

pub use paired::{create_host_pair, Host, Inbox, Outbox, Processor};

use paired::*;

use crate::{graph_query::GraphQuery, gui::windows::OverlayCreatorMsg};

pub struct Reactor {
    thread_pool: futures::executor::ThreadPool,
    pub rayon_pool: Arc<rayon::ThreadPool>,

    pub graph_query: Arc<GraphQuery>,

    pub overlay_create_tx: Sender<OverlayCreatorMsg>,
    pub overlay_create_rx: Receiver<OverlayCreatorMsg>,
}

impl Reactor {
    pub fn init(
        thread_pool: futures::executor::ThreadPool,
        rayon_pool: rayon::ThreadPool,
        graph_query: Arc<GraphQuery>,
    ) -> Self {
        let overlay = crossbeam::channel::unbounded::<OverlayCreatorMsg>();

        let rayon_pool = Arc::new(rayon_pool);

        Self {
            thread_pool,
            rayon_pool,

            graph_query,

            overlay_create_tx: overlay.0,
            overlay_create_rx: overlay.1,
        }
    }

    pub fn create_host<F, I, T>(&mut self, func: F) -> Host<I, T>
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
                eprintln!("spawning reactor task");
                log::debug!("spawning reactor task");

                loop {
                    let _result = processor.process().await;
                }
            })
            .expect("Error when spawning reactor task");

        host
    }

    /*
    pub fn spawn_interval<F>(
        &mut self,
        func: F,
        dur: std::time::Duration,
    ) -> anyhow::Result<RemoteHandle<()>>
    where
        F: Fn(f64) + Send + Sync + 'static,
    {
        use futures_timer::Delay;
        use std::time::{Duration, SystemTime};

        let result = self.thread_pool.spawn_with_handle(async move {
            let looper = || {
                let delay = Delay::new(dur);
                async {
                    delay.await;
                    let t = SystemTime::now()
                        .duration_since(SystemTime::UNIX_EPOCH)
                        .unwrap_or(Duration::from_secs_f64(0.0))
                        .as_secs_f64();
                    func(t);
                }
            };

            loop {
                looper().await;
            }
        })?;
        Ok(result)
    }
    */

    pub fn spawn_interval<F>(
        &mut self,
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

    pub fn spawn<F, T>(&mut self, fut: F) -> anyhow::Result<RemoteHandle<T>>
    where
        F: Future<Output = T> + Send + Sync + 'static,
        T: Send + Sync + 'static,
    {
        let handle = self.thread_pool.spawn_with_handle(fut)?;
        Ok(handle)
    }
}
