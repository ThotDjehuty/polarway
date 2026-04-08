//! # Subscriber types — sync drain + async recv
//!
//! Two flavors:
//! - `Subscriber<T>`: unfiltered, receives everything
//! - `FilteredSubscriber<T, F>`: applies `F: Fn(&T) -> bool` before buffering

use tokio::sync::broadcast;
use log::warn;

use crate::error::BusError;
use crate::BusResult;

/// An unfiltered subscriber that accumulates events for sync drain.
pub struct Subscriber<T: Clone + Send + Sync + 'static> {
    rx: broadcast::Receiver<T>,
    buffer: Vec<T>,
}

impl<T: Clone + Send + Sync + 'static> Subscriber<T> {
    pub(crate) fn new(rx: broadcast::Receiver<T>) -> Self {
        Self {
            rx,
            buffer: Vec::new(),
        }
    }

    /// Drain all pending events (non-blocking).
    ///
    /// Internally calls `try_recv()` until the channel is empty,
    /// returns a `Vec<T>` of all accumulated events.
    pub fn drain(&mut self) -> Vec<T> {
        loop {
            match self.rx.try_recv() {
                Ok(event) => self.buffer.push(event),
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    warn!("subscriber lagged, missed {n} events");
                    // Continue draining what's still available
                }
                Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }
        std::mem::take(&mut self.buffer)
    }

    /// Async receive: wait for the next event.
    pub async fn recv(&mut self) -> BusResult<T> {
        match self.rx.recv().await {
            Ok(event) => Ok(event),
            Err(broadcast::error::RecvError::Lagged(n)) => Err(BusError::Lagged(n)),
            Err(broadcast::error::RecvError::Closed) => Err(BusError::Closed),
        }
    }

    /// Try to receive without waiting.
    pub fn try_recv(&mut self) -> BusResult<T> {
        match self.rx.try_recv() {
            Ok(event) => Ok(event),
            Err(broadcast::error::TryRecvError::Empty) => {
                Err(BusError::Other("no events pending".into()))
            }
            Err(broadcast::error::TryRecvError::Lagged(n)) => Err(BusError::Lagged(n)),
            Err(broadcast::error::TryRecvError::Closed) => Err(BusError::Closed),
        }
    }

    /// Number of events currently buffered (from previous partial drains).
    pub fn buffered_count(&self) -> usize {
        self.buffer.len()
    }
}

/// A filtered subscriber that only buffers events passing a predicate.
pub struct FilteredSubscriber<T: Clone + Send + Sync + 'static, F: Fn(&T) -> bool> {
    rx: broadcast::Receiver<T>,
    filter: F,
    buffer: Vec<T>,
}

impl<T: Clone + Send + Sync + 'static, F: Fn(&T) -> bool> FilteredSubscriber<T, F> {
    pub(crate) fn new(rx: broadcast::Receiver<T>, filter: F) -> Self {
        Self {
            rx,
            filter,
            buffer: Vec::new(),
        }
    }

    /// Drain all pending events that pass the filter (non-blocking).
    pub fn drain(&mut self) -> Vec<T> {
        loop {
            match self.rx.try_recv() {
                Ok(event) => {
                    if (self.filter)(&event) {
                        self.buffer.push(event);
                    }
                }
                Err(broadcast::error::TryRecvError::Empty) => break,
                Err(broadcast::error::TryRecvError::Lagged(n)) => {
                    warn!("filtered subscriber lagged, missed {n} events");
                }
                Err(broadcast::error::TryRecvError::Closed) => break,
            }
        }
        std::mem::take(&mut self.buffer)
    }

    /// Async receive: wait for the next event that passes the filter.
    pub async fn recv(&mut self) -> BusResult<T> {
        loop {
            match self.rx.recv().await {
                Ok(event) => {
                    if (self.filter)(&event) {
                        return Ok(event);
                    }
                    // Filtered out — keep waiting
                }
                Err(broadcast::error::RecvError::Lagged(n)) => return Err(BusError::Lagged(n)),
                Err(broadcast::error::RecvError::Closed) => return Err(BusError::Closed),
            }
        }
    }

    /// Number of events currently buffered.
    pub fn buffered_count(&self) -> usize {
        self.buffer.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::broadcast;

    #[tokio::test]
    async fn subscriber_drain_multiple() {
        let (tx, rx) = broadcast::channel::<i32>(64);
        let mut sub = Subscriber::new(rx);

        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();

        let events = sub.drain();
        assert_eq!(events, vec![1, 2, 3]);

        // Second drain should be empty
        let events2 = sub.drain();
        assert!(events2.is_empty());
    }

    #[tokio::test]
    async fn subscriber_async_recv() {
        let (tx, rx) = broadcast::channel::<&str>(64);
        let mut sub = Subscriber::new(rx);

        tx.send("hello").unwrap();
        let event = sub.recv().await.unwrap();
        assert_eq!(event, "hello");
    }

    #[tokio::test]
    async fn filtered_subscriber_drain() {
        let (tx, rx) = broadcast::channel::<i32>(64);
        let mut sub = FilteredSubscriber::new(rx, |n: &i32| *n > 2);

        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();
        tx.send(4).unwrap();

        let events = sub.drain();
        assert_eq!(events, vec![3, 4]);
    }

    #[tokio::test]
    async fn filtered_subscriber_async_recv() {
        let (tx, rx) = broadcast::channel::<i32>(64);
        let mut sub = FilteredSubscriber::new(rx, |n: &i32| *n % 2 == 0);

        // Spawn publisher
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            tx.send(1).unwrap(); // filtered out
            tx.send(2).unwrap(); // passes
        });

        let event = sub.recv().await.unwrap();
        assert_eq!(event, 2);
    }
}
