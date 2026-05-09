//! Concurrency utilities — MapReduce and functional pipeline (fx).
//!
//! Replicates go-zero's `mr` (MapReduce) and `fx` (functional stream) modules.

pub mod mr;
pub mod fx;
pub mod singleflight;

pub use mr::{map_reduce, MapResult};
pub use fx::{FxStream, from};
pub use singleflight::Group as SingleflightGroup;
