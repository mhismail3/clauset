//! Clauset server library - HTTP/WebSocket server for Claude Code session management.
//!
//! This library provides the HTTP routes, WebSocket handlers, and application state
//! for the Clauset dashboard server. It's separated from main.rs to enable integration testing.

pub mod config;
pub mod event_processor;
pub mod global_ws;
pub mod interaction_processor;
pub mod logging;
pub mod routes;
pub mod state;
pub mod websocket;
