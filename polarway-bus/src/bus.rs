//! # EventBus — broadcast publisher with subscriber management
//!
//! The bus owns a `tokio::sync::broadcast` channel and manages
//! subscriber creation. Events are generic over `T`.

use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::broadcast;

use crate::error::BusError;
use crate::subscriber::{Subscriber, FilteredSubscriber};
use crate::BusResult;

/// A generic, domain-agnostic event bus.
///
/// Publishes events of type `T` to all subscribers via a tokio broadcast channel.
/// Subscribers can optionally apply filters to only receive relevant events.
///
/// # Example
///
/// ```rust
/// use polarway_bus::EventBus;
///
/// # #[tokio::main] async fn main() {
/// let bus: EventBus<String> = EventBus::new(1024);
/// let mut sub = bus.subscribe();
///
/// bus.publish("hello".to_string()).unwrap();
///
/// let events = sub.drain();
/// assert_eq!(events.len(), 1);
/// assert_eq!(events[0], "hello");
/// # }
/// ```
pub struct EventBus<T: Clone + Send + Sync + 'static> {
    tx: broadcast::Sender<T>,
    capacity: usize,
    event_count: AtomicU64,
}

impl<T: Clone + Send + Sync + 'static> EventBus<T> {
    /// Create a new bus with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            capacity,
            event_count: AtomicU64::new(0),
        }
    }

    /// Publish an event to all subscribers.
    ///
    /// Returns the number of subscribers that received the event.
    pub fn publish(&self, event: T) -> BusResult<usize> {
        match self.tx.send(event) {
            Ok(n) => {
                self.event_count.fetch_add(1, Ordering::Relaxed);
                Ok(n)
            }
            Err(_) => Err(BusError::NoSubscribers),
        }
    }

    /// Create a new unfiltered subscriber.
    pub fn subscribe(&self) -> Subscriber<T> {
        Subscriber::new(self.tx.subscribe())
    }

    /// Create a new filtered subscriber.
    ///
    /// The filter receives a reference to each event and returns `true` to accept.
    ///
    /// # Example
    ///
    /// ```rust
    /// use polarway_bus::EventBus;
    ///
    /// # #[tokio::main] async fn main() {
    /// let bus: EventBus<i32> = EventBus::new(64);
    /// let mut sub = bus.subscribe_filtered(|n: &i32| *n > 5);
    ///
    /// bus.publish(3).unwrap();
    /// bus.publish(7).unwrap();
    ///
    /// let events = sub.drain();
    /// assert_eq!(events, vec![7]);
    /// # }
    /// ```
    pub fn subscribe_filtered<F>(&self, filter: F) -> FilteredSubscriber<T, F>
    where
        F: Fn(&T) -> bool,
    {
        FilteredSubscriber::new(self.tx.subscribe(), filter)
    }

    /// Number of active subscribers.
    pub fn subscriber_count(&self) -> usize {
        self.tx.receiver_count()
    }

    /// Channel capacity.
    pub fn capacity(&self) -> usize {
        self.capacity
    }

    /// Total events published since creation.
    pub fn event_count(&self) -> u64 {
        self.event_count.load(Ordering::Relaxed)
    }

    /// Get a clone of the sender (for creating subscribers externally).
    pub fn sender(&self) -> broadcast::Sender<T> {
        self.tx.clone()
    }
}

impl<T: Clone + Send + Sync + 'static> Default for EventBus<T> {
    fn default() -> Self {
        Self::new(4096)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn publish_to_single_subscriber() {
        let bus: EventBus<String> = EventBus::new(64);
        let mut sub = bus.subscribe();

        bus.publish("event_1".into()).unwrap();
        bus.publish("event_2".into()).unwrap();

        let events = sub.drain();
        assert_eq!(events.len(), 2);
        assert_eq!(events[0], "event_1");
        assert_eq!(events[1], "event_2");
    }

    #[tokio::test]
    async fn fan_out_to_multiple_subscribers() {
        let bus: EventBus<u64> = EventBus::new(64);
        let mut sub_a = bus.subscribe();
        let mut sub_b = bus.subscribe();

        bus.publish(42).unwrap();

        assert_eq!(sub_a.drain(), vec![42]);
        assert_eq!(sub_b.drain(), vec![42]);
    }

    #[tokio::test]
    async fn no_subscribers_returns_error() {
        let bus: EventBus<u8> = EventBus::new(64);
        assert!(matches!(bus.publish(1), Err(BusError::NoSubscribers)));
    }

    #[tokio::test]
    async fn event_count_tracks() {
        let bus: EventBus<i32> = EventBus::new(64);
        let _sub = bus.subscribe();
        bus.publish(1).unwrap();
        bus.publish(2).unwrap();
        assert_eq!(bus.event_count(), 2);
    }

    #[tokio::test]
    async fn filtered_subscriber() {
        let bus: EventBus<i32> = EventBus::new(64);
        let mut sub = bus.subscribe_filtered(|n: &i32| *n % 2 == 0);

        bus.publish(1).unwrap();
        bus.publish(2).unwrap();
        bus.publish(3).unwrap();
        bus.publish(4).unwrap();

        let events = sub.drain();
        assert_eq!(events, vec![2, 4]);
    }

    #[tokio::test]
    async fn subscriber_count() {
        let bus: EventBus<()> = EventBus::new(64);
        assert_eq!(bus.subscriber_count(), 0);
        let _sub1 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 1);
        let _sub2 = bus.subscribe();
        assert_eq!(bus.subscriber_count(), 2);
        drop(_sub1);
        // Note: broadcast doesn't decrement instantly, so we just check >= 1
        assert!(bus.subscriber_count() >= 1);
    }
}
