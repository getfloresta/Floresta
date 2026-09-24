// SPDX-License-Identifier: MIT OR Apache-2.0

use core::error::Error;
use core::fmt;
use core::fmt::Display;
use core::fmt::Formatter;

#[derive(PartialEq, Debug)]
pub enum WatchOnlyError {
    DatabaseError(String),
    DuplicateDescriptor { descriptor: String },
    InvalidDescriptor(String),
    InternalError(String),
}

impl Display for WatchOnlyError {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::DatabaseError(e) => {
                write!(f, "Database error: {e}")
            }
            Self::DuplicateDescriptor { descriptor } => {
                write!(f, "Descriptor is already cached: {descriptor}")
            }
            Self::InvalidDescriptor(e) => {
                write!(f, "Invalid descriptor: {e}")
            }
            Self::InternalError(e) => write!(f, "Internal Error: {e}"),
        }
    }
}

impl Error for WatchOnlyError {}
