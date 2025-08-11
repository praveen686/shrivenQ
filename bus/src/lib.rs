//! Lock-free event bus for ultra-low-latency message passing

#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]
#![deny(dead_code)]
#![deny(unused)]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

use anyhow::Result;
use crossbeam::channel;

/// Trait for messages that can be sent through the bus
pub trait Message: Send + Sync + 'static {}

/// Publisher trait for sending messages
pub trait Publisher<T: Message> {
    /// Publish a message to the bus
    fn publish(&self, msg: T) -> Result<()>;
}

/// Subscriber trait for receiving messages
pub trait Subscriber<T: Message> {
    /// Subscribe to receive messages
    fn subscribe(&self) -> Result<Receiver<T>>;
}

/// Receiver for messages from the bus
pub struct Receiver<T> {
    rx: channel::Receiver<T>,
}

impl<T> Receiver<T> {
    /// Receive a message, blocking if necessary
    pub fn recv(&self) -> Result<T> {
        Ok(self.rx.recv()?)
    }

    /// Try to receive a message without blocking
    pub fn try_recv(&self) -> Result<Option<T>> {
        match self.rx.try_recv() {
            Ok(msg) => Ok(Some(msg)),
            Err(channel::TryRecvError::Empty) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }
}

/// Multi-producer multi-consumer bus
pub struct Bus<T: Message> {
    tx: channel::Sender<T>,
    rx: channel::Receiver<T>,
}

impl<T: Message + Clone> Bus<T> {
    /// Create a new bounded bus with specified capacity
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = channel::bounded(capacity);
        Self { tx, rx }
    }

    /// Create a new unbounded bus
    pub fn unbounded() -> Self {
        let (tx, rx) = channel::unbounded();
        Self { tx, rx }
    }

    /// Get a publisher for this bus
    pub fn publisher(&self) -> BusPublisher<T> {
        BusPublisher {
            tx: self.tx.clone(),
        }
    }

    /// Get a subscriber for this bus
    pub fn subscriber(&self) -> BusSubscriber<T> {
        BusSubscriber {
            rx: self.rx.clone(),
        }
    }
}

/// Publisher for the bus
#[derive(Clone)]
pub struct BusPublisher<T> {
    tx: channel::Sender<T>,
}

impl<T: Message> Publisher<T> for BusPublisher<T> {
    fn publish(&self, msg: T) -> Result<()> {
        self.tx.send(msg)?;
        Ok(())
    }
}

/// Subscriber for the bus
#[derive(Clone)]
pub struct BusSubscriber<T> {
    rx: channel::Receiver<T>,
}

impl<T: Message + Clone> Subscriber<T> for BusSubscriber<T> {
    fn subscribe(&self) -> Result<Receiver<T>> {
        Ok(Receiver {
            rx: self.rx.clone(),
        })
    }
}

/// Single-producer single-consumer channel
pub struct SpscChannel;

impl SpscChannel {
    /// Create a new bounded SPSC channel
    pub fn new<T: Send + 'static>(capacity: usize) -> (Sender<T>, Receiver<T>) {
        let (tx, rx) = channel::bounded(capacity);
        (Sender { tx }, Receiver { rx })
    }

    /// Create a new unbounded SPSC channel
    pub fn unbounded<T: Send + 'static>() -> (Sender<T>, Receiver<T>) {
        let (tx, rx) = channel::unbounded();
        (Sender { tx }, Receiver { rx })
    }
}

/// Sender for SPSC channel
pub struct Sender<T> {
    tx: channel::Sender<T>,
}

impl<T: Send + Sync + 'static> Sender<T> {
    /// Send a message
    pub fn send(&self, msg: T) -> Result<()> {
        self.tx.send(msg)?;
        Ok(())
    }

    /// Try to send a message without blocking
    pub fn try_send(&self, msg: T) -> Result<()> {
        self.tx.try_send(msg)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq)]
    struct TestMessage {
        id: u64,
        data: String,
    }

    impl Message for TestMessage {}

    #[test]
    fn test_spsc_channel() -> Result<()> {
        let (tx, rx) = SpscChannel::new::<TestMessage>(10);

        let msg = TestMessage {
            id: 1,
            data: "test".to_string(),
        };

        tx.send(msg.clone())?;
        let received = rx.recv()?;
        assert_eq!(msg, received);
        Ok(())
    }

    #[test]
    fn test_bus_pubsub() -> Result<()> {
        let bus = Bus::<TestMessage>::new(10);
        let pub1 = bus.publisher();
        let sub1 = bus.subscriber();
        let rx = sub1.subscribe()?;

        let msg = TestMessage {
            id: 42,
            data: "hello".to_string(),
        };

        pub1.publish(msg.clone())?;
        let received = rx.recv()?;
        assert_eq!(msg, received);
        Ok(())
    }
}
