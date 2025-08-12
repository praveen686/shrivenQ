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
    ///
    /// # Errors
    /// Returns an error if the channel is disconnected or full (for bounded channels)
    fn publish(&self, msg: T) -> Result<()>;
}

/// Subscriber trait for receiving messages
pub trait Subscriber<T: Message> {
    /// Subscribe to receive messages
    ///
    /// # Errors
    /// Returns an error if unable to create the receiver
    fn subscribe(&self) -> Result<Receiver<T>>;
}

/// Receiver for messages from the bus
pub struct Receiver<T> {
    rx: channel::Receiver<T>,
}

impl<T> Receiver<T> {
    /// Receive a message, blocking if necessary
    ///
    /// # Errors
    /// Returns an error if the channel is disconnected
    #[must_use = "ignoring received messages defeats the purpose"]
    pub fn recv(&self) -> Result<T> {
        Ok(self.rx.recv()?)
    }

    /// Try to receive a message without blocking
    ///
    /// # Errors
    /// Returns an error if the channel is disconnected
    #[must_use = "ignoring received messages defeats the purpose"]
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
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = channel::bounded(capacity);
        Self { tx, rx }
    }

    /// Create a new unbounded bus
    #[must_use]
    pub fn unbounded() -> Self {
        let (tx, rx) = channel::unbounded();
        Self { tx, rx }
    }

    /// Get a publisher for this bus
    #[must_use]
    pub fn publisher(&self) -> BusPublisher<T> {
        BusPublisher {
            tx: self.tx.clone(),
        }
    }

    /// Get a subscriber for this bus
    #[must_use]
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

/// Event bus for the trading engine
pub struct EventBus {
    /// Internal channel for events
    tx: channel::Sender<Event>,
    /// Receiver for events
    rx: channel::Receiver<Event>,
}

impl EventBus {
    /// Create a new event bus with specified capacity
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        let (tx, rx) = channel::bounded(capacity);
        Self { tx, rx }
    }

    /// Send an event
    ///
    /// # Errors
    /// Returns an error if the channel is disconnected or full
    pub fn send(&self, event: Event) -> Result<()> {
        self.tx.send(event)?;
        Ok(())
    }

    /// Receive an event
    ///
    /// # Errors
    /// Returns an error if the channel is disconnected
    #[must_use = "ignoring received events defeats the purpose"]
    pub fn recv(&self) -> Result<Event> {
        Ok(self.rx.recv()?)
    }
}

/// Event types for the trading system
#[derive(Clone, Debug)]
pub enum Event {
    /// Market data event
    MarketData {
        /// Symbol identifier
        symbol: u32,
        /// Bid price in ticks
        bid: i64,
        /// Ask price in ticks
        ask: i64,
        /// Timestamp in nanoseconds
        ts: u64,
    },
    /// Order event
    Order {
        /// Order ID
        id: u64,
        /// Symbol identifier
        symbol: u32,
        /// Side (0=Buy, 1=Sell)
        side: u8,
        /// Quantity in units
        qty: i64,
    },
    /// Fill event
    Fill {
        /// Order ID
        order_id: u64,
        /// Filled quantity
        qty: i64,
        /// Fill price
        price: i64,
        /// Timestamp in nanoseconds
        ts: u64,
    },
}

impl Message for Event {}

/// Single-producer single-consumer channel
pub struct SpscChannel;

impl SpscChannel {
    /// Create a new bounded SPSC channel
    #[must_use]
    pub fn create<T: Send + 'static>(capacity: usize) -> (Sender<T>, Receiver<T>) {
        let (tx, rx) = channel::bounded(capacity);
        (Sender { tx }, Receiver { rx })
    }

    /// Create a new unbounded SPSC channel
    #[must_use]
    pub fn create_unbounded<T: Send + 'static>() -> (Sender<T>, Receiver<T>) {
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
    ///
    /// # Errors
    /// Returns an error if the channel is disconnected
    pub fn send(&self, msg: T) -> Result<()> {
        self.tx.send(msg)?;
        Ok(())
    }

    /// Try to send a message without blocking
    ///
    /// # Errors
    /// Returns an error if the channel is disconnected or full
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
        let (tx, rx) = SpscChannel::create::<TestMessage>(10);

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
