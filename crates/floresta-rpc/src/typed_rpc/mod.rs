//! Module that holds methods and types to properly handle RPCs in floresta.
//!
//! Idiomatically, data schemas for the RPCs are expressed as types and we use that to take
//! advantage to be generic over transport and serialization spec.
//!
//! You can find signature definitions for our RPCs under `command_def`, and the types mentioned above under
//! `response`.

pub mod command_def;
pub mod response;
