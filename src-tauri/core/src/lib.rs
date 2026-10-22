//! Platform-independent domain logic for LifeOS Notch.
//!
//! # Why this crate exists
//!
//! The application crate links `tauri` and the `windows` crate. On this
//! machine that combination produces a binary the Rust test harness cannot
//! load (`STATUS_ENTRYPOINT_NOT_FOUND`, from the WinRT API-sets that the
//! `windows` crate imports), even though the application binary itself starts
//! normally.
//!
//! Rather than leave every unit test unverifiable, the domain logic — the
//! database, settings, timer, water tracking, tasks and lyrics parsing — lives
//! here with no Windows or GUI dependency. This crate's tests run anywhere.
//!
//! The source files are shared with the application crate through `#[path]`
//! rather than copied, so there is exactly one implementation. Both crates
//! compile the same files; the application re-exports these modules so its
//! own `crate::types` / `crate::db` paths keep working.

#[path = "../../src/types.rs"]
pub mod types;

#[path = "../../src/error.rs"]
pub mod error;

#[path = "../../src/db.rs"]
pub mod db;

pub mod services;
