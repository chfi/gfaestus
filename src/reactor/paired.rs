use futures::future::BoxFuture;
use parking_lot::Mutex;
use std::sync::Arc;

use futures::channel::mpsc;
use futures::prelude::*;

pub fn create_host_pair<I, T>(
    func: Box<dyn Fn(&Outbox<T>, I) -> T + Send + Sync + 'static>,
) -> (Host<I, T>, Processor<I, T>)
where
    I: Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    let (input_send, input_recv) = mpsc::channel(8);

    let (outbox, inbox) = create_box_pair::<T>();

    (
        Host { inbox, input_send },
        Processor {
            outbox,
            input_recv,
            func,
        },
    )
}

pub struct Host<I, T>
where
    I: Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    inbox: Inbox<T>,
    input_send: mpsc::Sender<I>,
}

pub struct Processor<I, T>
where
    I: Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    outbox: Outbox<T>,
    input_recv: mpsc::Receiver<I>,
    func: Box<dyn Fn(&Outbox<T>, I) -> T + Send + Sync + 'static>,
}

impl<I, T> Host<I, T>
where
    I: Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    pub fn call(&mut self, input: I) -> anyhow::Result<()> {
        futures::executor::block_on(async {
            self.input_send.send(input).await
        })?;

        Ok(())
    }

    pub fn take(&self) -> Option<T> {
        self.inbox.take()
    }
}

pub trait ProcTrait: Send + Sync + 'static {
    fn process(&mut self) -> BoxFuture<()>;
}

impl<I, T> ProcTrait for Processor<I, T>
where
    I: Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    fn process(&mut self) -> BoxFuture<()> {
        let future = async move {
            if let Some(input) = self.input_recv.next().await {
                let func = &self.func;
                let output = func(&self.outbox, input);
                self.outbox.insert_blocking(output);
            }
        };

        future.boxed()
    }
}

// this is basically an unbounded channel except worse, i guess
fn create_box_pair<T>() -> (Outbox<T>, Inbox<T>) {
    let value = Arc::new(Mutex::new(None));

    let outbox = Outbox {
        value: value.clone(),
    };
    let inbox = Inbox { value };

    (outbox, inbox)
}

pub struct Inbox<T> {
    value: Arc<Mutex<Option<T>>>,
}

pub struct Outbox<T> {
    value: Arc<Mutex<Option<T>>>,
}

impl<T> Inbox<T> {
    /// If the value is filled, consume and return the contents; otherwise returns None
    pub fn take(&self) -> Option<T> {
        self.value.try_lock().and_then(|mut v| v.take())
    }
}

impl<T> Outbox<T> {
    /// Block the thread and replace the value with
    pub fn insert_blocking(&self, value: T) {
        // log::warn!("enter guard");
        let mut guard = self.value.lock();
        *guard = Some(value);
        // log::warn!("leave guard");
    }

    pub fn try_insert(&self, value: T) -> Result<(), T> {
        if let Some(mut guard) = self.value.try_lock() {
            *guard = Some(value);
            Ok(())
        } else {
            Err(value)
        }
    }
}
