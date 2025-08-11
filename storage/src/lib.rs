//! Write-Ahead Log (WAL) for crash-safe persistence and deterministic replay

#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]
#![deny(dead_code)]
#![deny(unused)]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod events;
pub mod segment;
pub mod wal;

pub use events::*;
pub use wal::{Wal, WalEntry, WalIterator};
