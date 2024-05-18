use std::sync::Arc;

use async_recursion::async_recursion;
use tokio::task::JoinHandle;

use crate::task::Task;

struct TaskHandle {
    task: Task,
    handle: JoinHandle<()>,
}
struct _EventQueue {
    tid: tokio::sync::Mutex<usize>,
    tasks: tokio::sync::Mutex<Vec<Task>>,
    current: tokio::sync::Mutex<Option<TaskHandle>>,
}
impl _EventQueue {
    fn new() -> Self {
        Self {
            tid: tokio::sync::Mutex::new(1),
            tasks: tokio::sync::Mutex::new(Vec::new()),
            current: tokio::sync::Mutex::new(None),
        }
    }
}
async fn spawn_task(sq: Arc<_EventQueue>, task: &Task) -> JoinHandle<()> {
    let task = task.clone();
    tokio::spawn(async move {
        task.clone().await;
        add(sq.clone(), task).await;
        next(sq.clone()).await;
    })
}

#[async_recursion]
async fn next(sq: Arc<_EventQueue>) {
    let mut queue_guard = sq.tasks.lock().await; 
    let mut current_guard = sq.current.lock().await;
    *current_guard = None;

    if let Some(task) = queue_guard.pop() {
        // println!("task next start: {:?} {:?}", task.id, task.timestamp);
        let handle = spawn_task(sq.clone(), &task).await;
        *current_guard = Some(TaskHandle {
            task,
            handle
        });
    }
}
#[async_recursion]
async fn add(sq: Arc<_EventQueue>, mut task: Task) {
    let mut tid = sq.tid.lock().await;
    let mut queue_guard = sq.tasks.lock().await; 
    let mut current_guard = sq.current.lock().await;

    task.id = Some(*tid);
    *tid += 1;

    task.ready();
    if task.timestamp.is_none() {
        return;
    }

    let taskhandle = current_guard.take();

    match taskhandle {
        Some(t) => {
            let Some(cur_timestamp) = task.timestamp else { return; };
            let Some(new_timestamp) = t.task.timestamp else { return; };
            if cur_timestamp < new_timestamp {
                // println!("task abort: {:?} {:?}", t.task.id, t.task.timestamp);
                t.handle.abort();
                queue_guard.push(t.task);
            }
            else {
                *current_guard = Some(t);
            }
        },
        None => {}
    }
    queue_guard.push(task);
    queue_guard.sort_by(|a, b| {
        b.timestamp.unwrap().cmp(&a.timestamp.unwrap())
    });

    if current_guard.is_none() {
        match queue_guard.pop() {
            Some(task) => {
                // println!("task add start: {:?} {:?}", task.id, task.timestamp);
                let handle = spawn_task(sq.clone(), &task).await;
                *current_guard = Some(TaskHandle {
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
    pub fn new() -> Self {
        Self {
            eq: Arc::new(_EventQueue::new())
        }
    }
    pub async fn add(&self, task: Task) {
        add(self.eq.clone(), task).await;
    }
}
