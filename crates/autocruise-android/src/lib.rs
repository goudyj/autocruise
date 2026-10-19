//! Drive the user's own Android apps over adb.
//!
//! Some services have no API and no usable website: only a mobile app. For
//! those, autocruise drives the **official app**, logged in with the user's
//! own account, on an Android device the user controls (a redroid container
//! or a real phone). Nothing is disguised: the app and its own protections run
//! unmodified; challenges are left to the user (see `docs/PRINCIPLES.md`).
//!
//! - [`adb`]: the [`AdbTransport`] abstraction and the real [`CommandAdb`].
//! - [`mock`]: [`mock::MockAdb`], canned adb outputs for tests.
//! - [`ui`]: the UI tree (uiautomator dump) and [`By`] selectors.
//! - [`device`]: [`Device`], high-level actions (launch, read UI, tap, type).
//! - [`error`]: [`AndroidError`], classified for the tool error contract.
//!
//! This crate knows no service. Connectors build on [`Device`] and keep
//! their app-specific selectors and flows to themselves.

pub mod adb;
pub mod device;
pub mod error;
pub mod mock;
pub mod ui;

pub use adb::{AdbTransport, CommandAdb};
pub use device::{Device, Key};
pub use error::{AdbError, AndroidError};
pub use ui::{Bounds, By, Point, UiNode, UiTree};
