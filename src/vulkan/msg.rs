use anyhow::Result;
use ash::{
    extensions::khr::{Surface, Swapchain},
    version::DeviceV1_0,
    vk, Device, Entry,
};

use bytemuck::{Pod, Zeroable};
use futures::Future;
use parking_lot::{Mutex, MutexGuard};
use std::{mem::size_of, sync::Arc};
use vk_mem::Allocator;

#[cfg(target_os = "linux")]
use winit::platform::unix::*;
use winit::{
    event_loop::EventLoop,
    window::{Window, WindowBuilder},
};

#[allow(unused_imports)]
use log::{debug, error, info, trace, warn};

use crate::{app::Args, view::ScreenDims};

use super::GfaestusVk;

pub struct GpuTasks {
    task_rx: crossbeam::channel::Receiver<TaskPkg>,
    task_tx: crossbeam::channel::Sender<TaskPkg>,
    // tasks: Vec<TaskPkg>,
}

impl std::default::Default for GpuTasks {
    fn default() -> Self {
        let (task_tx, task_rx) = crossbeam::channel::unbounded();
        GpuTasks { task_tx, task_rx }
    }
}

impl GpuTasks {
    // pub fn queue_task(&self, task: GpuTask) -> impl Future<Output = ()> {
    pub fn queue_task(
        &self,
        task: GpuTask,
    ) -> Result<futures::channel::oneshot::Receiver<()>> {
        let (tx, rx) = futures::channel::oneshot::channel::<()>();
        let task_pkg = TaskPkg { task, signal: tx };
        self.task_tx.send(task_pkg)?;
        Ok(rx)
    }

    pub fn execute_all(
        &self,
        app: &GfaestusVk,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> Result<()> {
        while let Ok(TaskPkg { task, signal }) = self.task_rx.try_recv() {
            let task_res = execute_task(app, command_pool, queue, task);

            if let Err(msg) = task_res {
                log::error!("GPU task error: {:?}", msg);
            }

            let signal_res = signal.send(());

            if let Err(msg) = signal_res {
                log::error!("GPU task signal error: {:?}", msg);
            }
        }

        Ok(())
    }

    pub fn execute_next(
        &self,
        app: &GfaestusVk,
        command_pool: vk::CommandPool,
        queue: vk::Queue,
    ) -> Result<()> {
        if let Ok(TaskPkg { task, signal }) = self.task_rx.try_recv() {
            let task_res = execute_task(app, command_pool, queue, task);

            if let Err(msg) = task_res {
                log::error!("GPU task error: {:?}", msg);
            }

            let signal_res = signal.send(());

            if let Err(msg) = signal_res {
                log::error!("GPU task signal error: {:?}", msg);
            }
        }

        Ok(())
    }
}

pub struct TaskPkg {
    task: GpuTask,
    signal: futures::channel::oneshot::Sender<()>,
}

pub enum GpuTask {
    CopyDataToBuffer {
        data: Arc<Vec<u32>>,
        dst: vk::Buffer,
    },
    // CopyImageToBuffer { image: vk::Image, buffer: vk::Buffer, extent: vk::Extent2D },
}

pub fn execute_task(
    app: &GfaestusVk,
    command_pool: vk::CommandPool,
    queue: vk::Queue,
    task: GpuTask,
) -> Result<()> {
    match task {
        GpuTask::CopyDataToBuffer { data, dst } => {
            app.copy_data_to_buffer::<u32, u32>(&data, dst)
        }
    }
}
