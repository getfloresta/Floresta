// SPDX-License-Identifier: MIT OR Apache-2.0

//! Daemonization support for `florestad`.
//!
//! This module provides a builder for forking `florestad` into a
//! background daemon process by implementing a minimal subset of
//! [`Daemonize`], using the [`libc`] crate.
//!
//! # Example
//!
//! ```no_run
//! use crate::daemonize::Daemon;
//!
//! Daemon::new("/var/lib/florestad")
//!     .pid_file("/var/run/florestad.pid")
//!     .fork()
//!     .expect("failed to daemonize");
//! ```
//!
//! [`Daemonize`]: https://docs.rs/daemonize
//! [`libc`]: libc

use std::env;
use std::fs;
use std::fs::File;
use std::io;
use std::os::fd::IntoRawFd;
use std::path::PathBuf;
use std::process;

/// PID file creation mask:
///   - Owner can read/write/execute
///   - Group can read/execute
///   - Others have no permissions
const MASK_OCTAL: libc::mode_t = 0o027;

/// Path to the null device.
const DEV_NULL: &str = "/dev/null";

/// Builder for daemonizing the `florestad` process.
pub struct Daemon {
    working_directory: PathBuf,
    pid_file: Option<PathBuf>,
}

impl Daemon {
    /// Instantiate a new [`Daemon`] builder.
    pub fn new(working_directory: impl Into<PathBuf>) -> Self {
        Self {
            working_directory: working_directory.into(),
            pid_file: None,
        }
    }

    /// Set a custom path for the PID file.
    ///
    /// If not set, defaults to `<working_directory/florestad.pid`.
    pub fn pid_file(mut self, pid_file: impl Into<PathBuf>) -> Self {
        self.pid_file = Some(pid_file.into());
        self
    }

    /// Fork the current process into a background daemon.
    pub fn fork(self) -> io::Result<()> {
        // Fork and exit the parent, leaving the child orphaned.
        match unsafe { libc::fork() } {
            -1 => return Err(io::Error::last_os_error()),
            0 => (),
            _pid => process::exit(0),
        }

        // Create a new session with `setsid`, detaching from the controlling terminal.
        if unsafe { libc::setsid() } == -1 {
            return Err(io::Error::last_os_error());
        }

        // Set the working directory.
        env::set_current_dir(&self.working_directory)?;
        // Set the file creation mask.
        unsafe { libc::umask(MASK_OCTAL) };

        // Redirect `stdin` to `/dev/null`.
        //
        // Since logging is handled by `tracing_subscriber`, we can
        // skip redirecting `stdout` and `stderr` to the log file.
        let null_stdin = File::open(DEV_NULL)?;
        if unsafe { libc::dup2(null_stdin.into_raw_fd(), libc::STDIN_FILENO) } == -1 {
            return Err(io::Error::last_os_error());
        }

        // Write the PID file to `Daemon.pid_file`, or
        // fallback to `<Daemon.working_directory>/florestad.pid`.
        let pid_file = self
            .pid_file
            .unwrap_or_else(|| self.working_directory.join("florestad.pid"));
        fs::write(pid_file, process::id().to_string())?;

        Ok(())
    }
}
