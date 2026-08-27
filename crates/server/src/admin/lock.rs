//! Who holds the data directory, and why the answer had to be invented here.
//!
//! `rn-site admin` opens hiqlite's data directory directly, and G21.1 says it
//! must refuse when a node is already holding it — *naming the pid*. hiqlite
//! cannot answer that question. Its own lock is a zero-byte file at
//! `<data_dir>/state_machine/lock`, created on open and removed on a clean
//! shutdown, and it carries no pid.
//!
//! Worse than useless, in fact: this build enables hiqlite's `auto-heal`
//! feature, and `StateMachineSqlite::check_set_lock_file` reads
//!
//! ```text
//! Lock file already exists: {path}
//! Node did not shut down gracefully - auto-rebuilding State Machine
//! ```
//!
//! and then **deletes the state-machine directory** so it can be rebuilt from
//! the raft log. That is the right behaviour after a crash and a catastrophe
//! for a second process opening a directory the first one is still serving
//! from. So the guard cannot be advisory and cannot be hiqlite's: it has to
//! run *before* the database is opened at all, and it has to be ours.
//!
//! Hence [`PidFile`]. The serving process writes its own pid into
//! `<data_dir>/rn-site.pid` immediately before it opens the database and
//! removes it after hiqlite has been handed back; the subcommand reads it
//! first and refuses if the process named in it is alive. A file naming a pid
//! that has exited is stale and is ignored — a killed node must not lock its
//! own operator out of the recovery tool.

use std::path::{Path, PathBuf};

use super::AdminError;

/// What the serving process writes and the subcommand reads.
pub const PID_FILENAME: &str = "rn-site.pid";

/// A pid file this process owns and removes when it is dropped.
#[derive(Debug)]
pub struct PidFile(PathBuf);

impl PidFile {
    /// Claim `<dir>/rn-site.pid` for this process.
    ///
    /// Returns `None` when the file cannot be written — a read-only directory
    /// is a problem the database open is about to report far better than this
    /// would, and refusing to serve over an unwritable *advisory* file would
    /// be the guard taking the deployment down.
    #[must_use]
    pub fn claim(dir: &Path) -> Option<Self> {
        let path = dir.join(PID_FILENAME);
        match std::fs::write(&path, std::process::id().to_string()) {
            Ok(()) => Some(Self(path)),
            Err(error) => {
                tracing::warn!(
                    path = %path.display(),
                    %error,
                    "could not write the pid file; `rn-site admin` will not be able to see this \
                     process and must not be run against this directory while it serves"
                );
                None
            }
        }
    }
}

impl Drop for PidFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// Refuse if a live process holds `dir`.
///
/// # Errors
///
/// [`AdminError::Locked`], naming the pid.
pub fn refuse_if_held(dir: &Path) -> Result<(), AdminError> {
    if let Some(pid) = holder(dir) {
        return Err(AdminError::Locked {
            pid,
            dir: dir.display().to_string(),
        });
    }
    Ok(())
}

/// The pid holding `dir`, if one is alive.
#[must_use]
pub fn holder(dir: &Path) -> Option<u32> {
    let raw = std::fs::read_to_string(dir.join(PID_FILENAME)).ok()?;
    let pid: u32 = raw.trim().parse().ok()?;
    alive(pid).then_some(pid)
}

/// Whether a pid names a live process.
///
/// `kill(pid, 0)` is the portable ask-do-not-send form: it reports whether the
/// process exists rather than signalling it. `EPERM` counts as alive — a
/// process this user may not signal is still a process holding the directory,
/// and answering "gone" there is the one wrong answer this function can give.
#[cfg(unix)]
fn alive(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: `kill` with signal 0 sends nothing. It reads the process table
    // and returns; there is no memory involved and no state changed.
    if unsafe { libc_kill(pid, 0) } == 0 {
        return true;
    }
    std::io::Error::last_os_error().kind() == std::io::ErrorKind::PermissionDenied
}

#[cfg(not(unix))]
fn alive(_pid: u32) -> bool {
    // Nothing here runs off unix. Answering "alive" is the safe direction: it
    // refuses rather than opening a directory somebody may be serving from.
    true
}

#[cfg(unix)]
unsafe extern "C" {
    #[link_name = "kill"]
    fn libc_kill(pid: i32, sig: i32) -> i32;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_absent_file_is_not_a_lock() {
        let dir = tempfile::tempdir().expect("a directory");
        assert!(holder(dir.path()).is_none());
        refuse_if_held(dir.path()).expect("nothing holds it");
    }

    #[test]
    fn this_process_holds_what_it_claimed_and_lets_go_when_dropped() {
        let dir = tempfile::tempdir().expect("a directory");
        let claimed = PidFile::claim(dir.path()).expect("a writable directory");
        assert_eq!(holder(dir.path()), Some(std::process::id()));
        let refused = refuse_if_held(dir.path()).expect_err("a live holder");
        assert!(
            refused
                .to_string()
                .contains(&std::process::id().to_string()),
            "the refusal must name the pid: {refused}"
        );
        drop(claimed);
        assert!(
            holder(dir.path()).is_none(),
            "the file goes with the process"
        );
    }

    /// A killed node must not lock its operator out of the tool that recovers
    /// it, so a pid file naming a process that has exited is ignored.
    #[test]
    fn a_stale_pid_is_not_a_lock() {
        let dir = tempfile::tempdir().expect("a directory");
        // A pid one past the maximum any system will have issued. Not "some
        // large number and hope": `kill` on it cannot find a process.
        std::fs::write(dir.path().join(PID_FILENAME), "4294967294").expect("the file");
        assert!(holder(dir.path()).is_none());

        std::fs::write(dir.path().join(PID_FILENAME), "not a pid").expect("the file");
        assert!(holder(dir.path()).is_none(), "junk is not a lock either");
    }
}
