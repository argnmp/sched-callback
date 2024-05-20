use std::{future::Future, pin::Pin, sync::Arc, task::{Poll, Waker}, time::{Duration, SystemTime}};

type Callback = Box<dyn Fn() -> Pin<Box<dyn Future<Output = ()> + Send + 'static>> + Send + 'static>;
type ArcAsyncMutex<T> = Arc<tokio::sync::Mutex<T>>;
type ArcSyncMutex<T> = Arc<std::sync::Mutex<T>>;
type AsyncRt = tokio::runtime::Handle;

#[derive(Clone, Debug)]
pub enum SchedType {
    Timestamp(SystemTime),
    Delay(Duration, usize),
}

pub struct Task {
    pub(crate) id: Option<usize>,
    sched_type: SchedType,
    pub(crate) timestamp: Option<SystemTime>,
    callback: ArcAsyncMutex<Callback>,
    _waker: Option<ArcSyncMutex<Waker>>,
    _rt: Option<AsyncRt>,
}
impl Task {
    pub fn new(sched_type: SchedType, callback: Callback) -> Self {
        Self {
            id: None,
            sched_type,
            timestamp: None,
            callback: Arc::new(tokio::sync::Mutex::new(callback)),
            _waker: None,
            _rt: None,
        }
    }
    pub(crate) fn ready(&mut self) {
        match &mut self.sched_type {
            SchedType::Timestamp(timestamp) => {
                match self.timestamp {
                    Some(_) => self.timestamp = None,
                    None => self.timestamp = Some(*timestamp),
                }
            },
            SchedType::Delay(dur, count) => {
                match count {
                    0 => {
                        self.timestamp = None; 
                    },
                    _ => {
                        self.timestamp = Some(SystemTime::now() + *dur);
                        *count -= 1;
                    }
                }
            }
            
        }
        self._rt = Some(tokio::runtime::Handle::current());
    }
}
impl Clone for Task {
    fn clone(&self) -> Self {
        Self {
            id: self.id,
            sched_type: self.sched_type.clone(),
            timestamp: self.timestamp.clone(),
            callback: self.callback.clone(),
            _waker: self._waker.clone(),
            _rt: self._rt.clone(),
        }
    }
}
impl Future for Task {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> std::task::Poll<Self::Output> {
        let Some(handle) = self._rt.clone() else { return Poll::Ready(()); };
        let Some(next_timestamp) = self.timestamp else { return Poll::Ready(()); };
        if SystemTime::now() >= next_timestamp {
            let callback = self.callback.clone();
            handle.spawn(async move {
                let guard = callback.lock().await;
                guard().await;
            });
            return Poll::Ready(())
        }    
        if let Some(waker) = &self._waker {
            let mut waker = waker.lock().unwrap();
            if !waker.will_wake(cx.waker()) {
                *waker = cx.waker().clone();
            }
        } else {
            let waker = Arc::new(std::sync::Mutex::new(cx.waker().clone()));
            self._waker = Some(waker.clone());

            handle.spawn(async move {
                let current_time = SystemTime::now();
                if current_time < next_timestamp {
                    let diff = next_timestamp.duration_since(current_time).unwrap();
                    tokio::time::sleep(diff).await;
                }

                let waker = waker.lock().unwrap();
                waker.wake_by_ref();
            });
        }
        return Poll::Pending;
    }
}

