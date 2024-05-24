use std::sync::Arc;

use async_recursion::async_recursion;
use tokio::{sync::MutexGuard, task::JoinHandle};

use crate::task::Task;

pub(crate) type AsyncRt = tokio::runtime::Handle;

struct TaskHandle {
    task: Task,
    handle: JoinHandle<()>,
}
struct _EventQueue {
    tid: tokio::sync::Mutex<usize>,
    tasks: tokio::sync::Mutex<Vec<Task>>,
    running: tokio::sync::Mutex<Option<TaskHandle>>,
    _rt: AsyncRt,
}
impl _EventQueue {
    fn new(rt: AsyncRt) -> Self {
        Self {
            tid: tokio::sync::Mutex::new(1),
            tasks: tokio::sync::Mutex::new(Vec::new()),
            running: tokio::sync::Mutex::new(None),
            _rt: rt,
        }
    }
    async fn lock_tasks(&self) -> MutexGuard<Vec<Task>> {
        self.tasks.lock().await 
    }
    async fn lock_running(&self) -> MutexGuard<Option<TaskHandle>> {
        self.running.lock().await 
    }
    async fn get_tid(&self) -> usize {
        let mut guard = self.tid.lock().await; 
        let tid = *guard;
        *guard += 1;
        return tid;
    }
}
async fn _run_task(sq: Arc<_EventQueue>, task: &Task) -> JoinHandle<()> {
    let sqc = sq.clone();
    let task = task.clone();
    sq._rt.spawn(async move {
        task.clone().await;
        _add(sqc.clone(), task).await;
        _next(sqc.clone()).await;
    })
}

#[async_recursion]
async fn _next(sq: Arc<_EventQueue>) {
    let mut tasks_guard = sq.lock_tasks().await; 
    let mut running_guard = sq.lock_running().await;
    *running_guard = None;

    if let Some(task) = tasks_guard.pop() {
        let handle = _run_task(sq.clone(), &task).await;
        *running_guard = Some(TaskHandle {
            task,
            handle
        });
    }
}
#[async_recursion]
async fn _add(sq: Arc<_EventQueue>, mut task: Task) {
    let mut tasks_guard = sq.tasks.lock().await; 
    let mut running_guard = sq.running.lock().await;

    task.id = Some(sq.get_tid().await);
    task.ready(sq._rt.clone());

    if task.timestamp.is_none() {
        return;
    }

    let taskhandle = running_guard.take();

    match taskhandle {
        Some(t) => {
            let Some(cur_timestamp) = task.timestamp else { return; };
            let Some(new_timestamp) = t.task.timestamp else { return; };
            if cur_timestamp < new_timestamp {
                t.handle.abort();
                tasks_guard.push(t.task);
            }
            else {
                *running_guard = Some(t);
            }
        },
        None => {}
    }
    tasks_guard.push(task);
    tasks_guard.sort_by(|a, b| {
        b.timestamp.unwrap().cmp(&a.timestamp.unwrap())
    });

    if running_guard.is_none() {
        match tasks_guard.pop() {
            Some(task) => {
                let handle = _run_task(sq.clone(), &task).await;
                *running_guard = Some(TaskHandle {
                    task,
                    handle
                });
            },
            None => {

            }
        }
    }
}

pub struct SchedQueue {
    eq: Arc<_EventQueue>,
}
impl SchedQueue {
    /// Create new scheduler.
    /// Use tokio runtime of the context of this function is called.
    pub fn new() -> Self {
        Self {
            eq: Arc::new(_EventQueue::new(tokio::runtime::Handle::current()))
        }
    }
    /// Schedule task.
    /// After calling this function, queue will automatically start scheduling tasks.
    pub async fn add(&self, task: Task) {
        _add(self.eq.clone(), task).await;
    }
}
