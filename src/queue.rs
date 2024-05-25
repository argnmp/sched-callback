use std::{collections::BTreeSet, sync::Arc, time::SystemTime};

use async_recursion::async_recursion;
use tokio::{sync::{mpsc, MutexGuard}, task::JoinHandle};

use crate::{message::MessageType, task::Task};

pub(crate) type AsyncRt = tokio::runtime::Handle;

struct TaskHandle {
    task: Task,
    handle: JoinHandle<()>,
}
struct _EventQueue {
    tid: tokio::sync::Mutex<usize>,
    tasks: tokio::sync::Mutex<Vec<Task>>,
    running: tokio::sync::Mutex<Option<TaskHandle>>,
    tasks_cancelled: tokio::sync::Mutex<BTreeSet<usize>>,
    tx: mpsc::Sender<MessageType>,
    _rt: AsyncRt,
}
impl _EventQueue {
    fn new(rt: AsyncRt, tx: mpsc::Sender<MessageType>) -> Self {
        Self {
            tid: tokio::sync::Mutex::new(1),
            tasks: tokio::sync::Mutex::new(Vec::new()),
            running: tokio::sync::Mutex::new(None),
            tasks_cancelled: tokio::sync::Mutex::new(BTreeSet::new()),
            tx,
            _rt: rt,
        }
    }
    async fn lock_tasks(&self) -> MutexGuard<Vec<Task>> {
        self.tasks.lock().await 
    }
    async fn lock_running(&self) -> MutexGuard<Option<TaskHandle>> {
        self.running.lock().await 
    }
    async fn lock_tasks_cancelled(&self) -> MutexGuard<BTreeSet<usize>>{
        self.tasks_cancelled.lock().await
    }
    async fn get_tid(&self) -> usize {
        let mut guard = self.tid.lock().await; 
        let tid = *guard;
        *guard += 1;
        tid
    }
}
async fn _run_task(sq: Arc<_EventQueue>, task: &Task) -> Option<JoinHandle<()>> {
    let sqc = sq.clone();
    let task = task.clone();
    let task_id = task.id.expect("_run_task id not assigned to task");
    // check task is cancelled
    let mut tasks_cancelled_guard = sq.lock_tasks_cancelled().await;
    if tasks_cancelled_guard.remove(&task.id.unwrap()) {
        sqc.tx.send(MessageType::Cancel(task_id, SystemTime::now())).await.expect("_run_task message sending failed");
        return None;
    }
    
    Some(sq._rt.spawn(async move {
        sqc.tx.send(MessageType::WaitStart(task_id, SystemTime::now())).await.expect("_run_task message sending failed");
        task.clone().await;
        sqc.tx.send(MessageType::WaitEnd(task_id, SystemTime::now())).await.expect("_run_task message sending failed");
        _add(sqc.clone(), task).await;
        _next(sqc.clone()).await;
    }))
}

#[async_recursion]
async fn _next(sq: Arc<_EventQueue>) {
    let mut tasks_guard = sq.lock_tasks().await; 
    let mut running_guard = sq.lock_running().await;
    *running_guard = None;

    if let Some(task) = tasks_guard.pop() {
        let handle = _run_task(sq.clone(), &task).await;
        if let Some(handle) = handle {
            *running_guard = Some(TaskHandle {
                task,
                handle
            });
        }
    }
}
#[async_recursion]
async fn _add(sq: Arc<_EventQueue>, mut task: Task) -> Option<usize> {
    let mut tasks_guard = sq.tasks.lock().await; 
    let mut running_guard = sq.running.lock().await;

    // assign task id if it's None
    let task_id: usize;
    match task.id {
        Some(id) => task_id = id,
        None => {
            task_id = sq.get_tid().await;
            task.id = Some(task_id);
        }

    }
    
    // initialize timestamp of task
    task.ready(sq._rt.clone());
    
    // if timestamp is None, no more schedule is need for the task
    match task.timestamp {
        Some(_) => {
            sq.tx.send(MessageType::Add(task_id, SystemTime::now())).await.expect("_add: message sending failed");
        },
        None => {
            return None;
        }
    }

    let taskhandle = running_guard.take();
    if let Some(t) = taskhandle {
        let Some(running_timestamp) = t.task.timestamp else { return None; };
        let Some(new_timestamp) = task.timestamp else { return None; };

        // if new task timestamp is earlier than the running one, abort waiting task;
        if new_timestamp < running_timestamp {
            t.handle.abort();
            tasks_guard.push(t.task);
            sq.tx.send(MessageType::Abort(task_id, SystemTime::now())).await.expect("_add: message sending failed");
        }
        else {
            *running_guard = Some(t);
        }
    }

    tasks_guard.push(task);
    tasks_guard.sort_by(|a, b| {
        b.timestamp.unwrap().cmp(&a.timestamp.unwrap())
    });

    if running_guard.is_none() {
        if let Some(task) = tasks_guard.pop() {
            let handle = _run_task(sq.clone(), &task).await;
            if let Some(handle) = handle {
                *running_guard = Some(TaskHandle {
                    task,
                    handle
                });
            }
        }
    }

    return Some(task_id);
}

async fn _cancel(sq: Arc<_EventQueue>, id: usize) -> bool {
    let mut tasks_cancelled_guard = sq.lock_tasks_cancelled().await;
    tasks_cancelled_guard.insert(id)
}

#[derive(Clone)]
pub struct SchedQueue {
    eq: Arc<_EventQueue>,
}
impl SchedQueue {
    /// Create new scheduler.
    /// Use tokio runtime of the context that this function is called.
    pub fn new() -> (Self, mpsc::Receiver<MessageType>) {
        let (tx, rx) = mpsc::channel(1000);
        (Self {
            eq: Arc::new(_EventQueue::new(tokio::runtime::Handle::current(), tx)),
        }, rx)
    }
    /// Schedule task.
    /// After calling this function, scheduler will automatically start executing its tasks.
    pub async fn add(&self, task: Task) -> Option<usize> {
        _add(self.eq.clone(), task).await
    }

    pub async fn cancel(&self, task_id: usize) -> bool {
        _cancel(self.eq.clone(), task_id).await
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::{Duration, SystemTime}};
    use tokio::sync::Mutex;
    use crate::{queue::SchedQueue, task::{SchedType, Task}};

    #[tokio::test]
    async fn timestamp_test() {
        let (sq, _rx) = SchedQueue::new();

        let reserved_time = SystemTime::now() + Duration::from_millis(500);
        let executed_time = Arc::new(Mutex::new(SystemTime::now()));
        
        let ex = executed_time.clone();
        let _ = sq.add(Task::new(SchedType::Timestamp(reserved_time), Box::new(move ||{
            let ex = ex.clone();
            Box::pin(async move {
                let mut guard = ex.lock().await;
                *guard = SystemTime::now();
            })
        }))).await;
        tokio::time::sleep(Duration::from_secs(1)).await;
        let guard = executed_time.lock().await;
        
        let diff = guard.duration_since(reserved_time).unwrap();
        // assert that difference is < 10ms
        assert!(diff < Duration::from_millis(10));
    }
    #[tokio::test]
    async fn delay_test() {
        let (sq, _rx) = SchedQueue::new();
        let order = Arc::new(Mutex::new(Vec::new()));

        for i in 0..10 {
            let order = order.clone();
            let _ = sq.add(Task::new(SchedType::Delay(Duration::from_millis(101-i), 2), Box::new(move || {
                let order = order.clone();
                Box::pin(async move {
                    let mut guard = order.lock().await;
                    guard.push(i);
                })
            }))).await;

        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        let guard = order.lock().await;
        let expected = [9,8,7,6,5,4,3,2,1,0,9,8,7,6,5,4,3,2,1,0];
        assert_eq!(guard.len(), expected.len());
        for (e, r) in expected.iter().zip(&*guard) {
            assert_eq!(e, r);
        }

    }
    #[tokio::test]
    async fn cancel_test() {
        let (sq, _rx) = SchedQueue::new();
        let order = Arc::new(Mutex::new(Vec::new()));

        for i in 0..10 {
            let order = order.clone();
            let sqc = sq.clone();
            let _ = sq.add(Task::new(SchedType::Delay(Duration::from_millis(101-i), 2), Box::new(move || {
                let order = order.clone();
                let sqc = sqc.clone();
                Box::pin(async move {
                    sqc.cancel((i+1).try_into().unwrap()).await;
                    let mut guard = order.lock().await;
                    guard.push(i);
                })
            }))).await;

        }
        tokio::time::sleep(Duration::from_secs(1)).await;
        let guard = order.lock().await;
        let expected = [9,8,7,6,5,4,3,2,1,0];
        assert_eq!(guard.len(), expected.len());
        for (e, r) in expected.iter().zip(&*guard) {
            assert_eq!(e, r);
        }
    }
}
