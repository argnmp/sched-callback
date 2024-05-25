use std::time::SystemTime;

#[derive(Debug)]
pub enum MessageType{
    Add(usize, SystemTime),
    Cancel(usize, SystemTime),
    WaitStart(usize, SystemTime),
    WaitEnd(usize, SystemTime), 
    Abort(usize, SystemTime),
    ExecuteCallback(usize, SystemTime),
}
#[cfg(test)]
mod tests {
    use std::{sync::Arc, time::{Duration}};
    use tokio::sync::Mutex;
    use crate::{queue::SchedQueue, task::{SchedType, Task}, message::MessageType};

    #[tokio::test]
    async fn message_test() {
        let (sq, mut rx) = SchedQueue::new();
        let order = Arc::new(Mutex::new(Vec::new()));

        for i in 0..10 {
            let order = order.clone();
            let _ = sq.add(Task::new(SchedType::Delay(Duration::from_millis(101-i), 1), Box::new(move || {
                let order = order.clone();
                Box::pin(async move {
                    let mut guard = order.lock().await;
                    guard.push(i);
                })
            }))).await;
        }
        let messages = Arc::new(Mutex::new(Vec::new()));
        let m = messages.clone();
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let mut guard = m.lock().await;
                guard.push(msg);
            }
        });
        tokio::time::sleep(Duration::from_secs(1)).await;
        let guard = messages.lock().await;
        let mut length = (0, 0, 0, 0);
        for msg in &*guard {
            match msg {
                MessageType::Add(_, _) => length.0 += 1,
                MessageType::WaitStart(_, _) => length.1 += 1,
                MessageType::WaitEnd(_, _) => length.2 += 1,
                MessageType::Abort(_, _) => length.3 += 1,
                _ => {},
            }
        }
        assert_eq!(length.0, 10);
        assert_eq!(length.1, 10);
        assert_eq!(length.2, 10);
        assert_eq!(length.3, 9);
    }
}
