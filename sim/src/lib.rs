//! Simulation and replay functionality for backtesting and analysis

#![deny(warnings)]
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![deny(clippy::cargo)]
#![deny(dead_code)]
#![deny(unused)]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

pub mod replay;

pub use replay::{ReplayConfig, ReplayStatus, Replayer};
