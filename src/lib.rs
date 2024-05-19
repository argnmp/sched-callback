//! # sched-callback
//! A scheduler that executes async callback at certain point.
//! 
//! ## Overview
//! - Works on tokio runtime.
//! - Lightweight scheduler that only one task is executed in one task queue.
//!
//! ## Usage
//! Create scheduler using `queue::SchedQueue`
//! ```rust,no_run
//! let sq = SchedQueue::new();
//! ```
//! 
//! Callback type:
//! ```rust,no_run
//! type Callback = Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> + Send + 'static>;
//! ````
//!
//! Add task with callback.
//! Callback will be triggered 1 second after the task is added, and will be rescheduled for 10 times after the callback has been triggered.
//! ```rust,no_run
//! sq.add(Task::new(SchedType::Delay(Duration::from_secs(1), 10), Box::new(move || {
//!     Box::pin(async move {
//!         println!("hello world");
//!     })
//! }))).await;
//! ````
//!
//! Two types of task can be added to queue. `SchedType::Timestamp(SystemTime)` specifies the exact
//! timestamp that the callback will be triggered at. `SchedType::Delay(Duration, usize)` specifies
//! when the callback will be triggered after the task is added and how many times will it be
//! rescheduled.

pub mod task;
pub mod queue;

#[cfg(test)]
mod test {
    use std::{sync::Arc, time::Duration};

    use tokio::sync::Mutex;

    use crate::{queue::SchedQueue, task::{SchedType, Task}};

    #[tokio::test]
    async fn main_test() {
        let sq = SchedQueue::new();

        let acc = Arc::new(Mutex::new(0));

        for i in 0..10000 {
            let acc = acc.clone();
            sq.add(Task::new(SchedType::Delay(Duration::from_millis(10001-i), 10), Box::new(move || {
                let acc = acc.clone();
                Box::pin(async move {
                    let mut guard = acc.lock().await;
                    *guard += 1;
                })

            }))).await;
        }
        tokio::time::sleep(Duration::from_secs(120)).await;
        let guard = acc.lock().await;
        dbg!(*guard);
    }
}
