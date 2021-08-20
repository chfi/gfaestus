use futures::task::SpawnExt;

mod paired;

pub use paired::{create_host_pair, Host, Inbox, Outbox, Processor};

use paired::*;

pub struct Reactor {
    thread_pool: futures::executor::ThreadPool,
}

impl Reactor {
    pub fn init(thread_pool: futures::executor::ThreadPool) -> Self {
        Self { thread_pool }
    }

    pub fn create_host<F, I, T>(&mut self, func: F) -> Host<I, T>
    where
        T: Send + Sync + 'static,
        I: Send + Sync + 'static,
        F: Fn(I) -> T + Send + Sync + 'static,
    {
        let boxed_func = Box::new(func) as Box<_>;

        let (host, proc) = create_host_pair(boxed_func);

        let processor =
            Box::new(proc) as Box<dyn ProcTrait + Send + Sync + 'static>;

        self.thread_pool
            .spawn(async move {
                processor.process().unwrap();
            })
            .expect("Error when spawning reactor task");

        host
    }
}
