//! Legacy module kept for tooling compatibility. Real implementation lives in
//! `line_impl.rs`; this file only re-exports the public API so that path-based
//! scripts continue to work.

pub use crate::line::line_impl::*;
