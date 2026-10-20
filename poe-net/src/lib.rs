//! Transport layer shared by the login and instance servers and the client DLL.
//!
//! The PoE stream has no length prefix: a message is an opcode followed by its fields, so
//! framing is the job of `poe-proto`. This crate only provides the byte-level pieces:
//! - [`codec`]: big-endian reader/writer with PoE string encodings,
//! - [`cipher`]: the [`cipher::StreamCipher`] abstraction (plain or encrypted streams),
//! - [`hexdump`]: logging helper,
//! - `conn` (cargo feature `tokio`, off by default): a buffered TCP connection that applies a
//!   cipher in both directions. Everything else works without tokio.

pub mod cipher;
pub mod codec;
#[cfg(feature = "tokio")]
pub mod conn;
pub mod hexdump;

pub use cipher::{KeyMaterial, Plain, Salsa20Stream, StreamCipher};
pub use codec::{DecodeError, Reader, Writer};
#[cfg(feature = "tokio")]
pub use conn::Connection;
