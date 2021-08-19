use futures::{prelude::*, Future, FutureExt};

use futures::executor;
use futures::task::{LocalSpawn, LocalSpawnExt, Spawn, SpawnExt};

use crossbeam::atomic::AtomicCell;
use crossbeam::channel;
use parking_lot::{Mutex, MutexGuard};
use std::sync::Arc;

pub fn create_host_pair<I, T>(
    func: Box<dyn Fn(I) -> T>,
) -> (Host<I, T>, Processor<I, T>)
where
    I: Send + Sync + 'static,
{
    let (input_send, input_recv) = channel::unbounded();

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
{
    inbox: Inbox<T>,
    input_send: channel::Sender<I>,
}

pub struct Processor<I, T>
where
    I: Send + Sync + 'static,
{
    outbox: Outbox<T>,
    input_recv: channel::Receiver<I>,
    func: Box<dyn Fn(I) -> T>,
}

impl<I, T> Host<I, T>
where
    I: Send + Sync + 'static,
{
    pub fn call(&self, input: I) -> anyhow::Result<()> {
        self.input_send.send(input)?;
        Ok(())
    }

    pub fn take(&self) -> Option<T> {
        self.inbox.take()
    }
}

impl<I, T> Processor<I, T>
where
    I: Send + Sync + 'static,
{
    pub fn process(&self) -> anyhow::Result<()> {
        while let input = self.input_recv.recv()? {
            let func = &self.func;
            let output = func(input);
            self.outbox.insert_blocking(output);
        }

        Ok(())
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
        let mut guard = self.value.lock();
        *guard = Some(value);
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
