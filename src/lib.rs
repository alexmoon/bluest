pub mod adapter;
pub mod characteristic;
mod delegates;
pub mod descriptor;
pub mod device;
pub mod error;
pub mod service;
pub mod session;
mod types;
pub mod uuid;

pub use smallvec;

pub use error::Error;
pub type Result<T, E = Error> = core::result::Result<T, E>;
