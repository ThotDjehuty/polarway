# polarway-bus

Generic, domain-agnostic event bus with broadcast fan-out, filtered subscribers, and sync drain buffers.

## Features

- **Broadcast fan-out** — every subscriber receives every event via `tokio::sync::broadcast`
- **Filtered subscribers** — register closure predicates to drop irrelevant events
- **Sync drain** — batch-drain all pending events without async (`sub.drain()`)
- **Async recv** — `sub.recv().await` for event-loop integration
- **Generic** — works with any `T: Clone + Send + Sync + 'static`

## Usage

```rust
use polarway_bus::EventBus;

#[tokio::main]
async fn main() {
    let bus: EventBus<String> = EventBus::new(4096);
    let mut sub = bus.subscribe();

    bus.publish("hello".to_string()).unwrap();

    let events = sub.drain();
    assert_eq!(events, vec!["hello".to_string()]);
}
```

## License

MIT
