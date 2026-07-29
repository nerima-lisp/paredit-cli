#![doc = include_str!("../README.md")]

pub mod message;
pub mod transport;

pub use message::{Handler, Outcome, Request, ResponseError, error_codes};
pub use transport::{Framing, Reader, Writer, serve_stdio};
