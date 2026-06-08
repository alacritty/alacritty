//! AI chat panel.
//!
//! A chat assistant docked below the terminal grid. It can read the visible screen and
//! recent scrollback, talk to an OpenAI-compatible API, and run shell commands on the
//! user's behalf (subject to an approval policy).
//!
//! The API key is never stored in the config file; it lives in the OS keyring and is
//! accessed through [`secret`].

pub mod agent;
pub mod approval;
pub mod client;
pub mod panel;
pub mod secret;
