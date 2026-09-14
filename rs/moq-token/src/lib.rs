//! JWT token generation and validation for MoQ authentication.
//!
//! Create and verify JWT tokens used for authorizing publish/subscribe operations in MoQ.
//! Tokens specify which broadcast paths a client can publish to and consume from.
//!
//! See [`Claims`] for the JWT claims structure and [`Key`] for key management.
//! Pattern types from [`moq-pattern`](moq_pattern) are re-exported for standalone use.
//! Unversioned claims use v0 path prefixes; `v: 1` claims use exact patterns.

mod algorithm;
mod claims;
mod error;
mod fs;
mod generate;
mod key;
mod key_id;
mod path;
mod set;

pub use algorithm::*;
pub use claims::*;
pub use error::*;
pub use key::*;
pub use key_id::*;
pub use moq_pattern::{InvalidPattern, Pattern, Patterns, Segment, Specificity};
pub use set::*;
