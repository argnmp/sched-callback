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
pub mod message;
