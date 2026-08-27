//! When a field talks to the server.
//!
//! There is no save button. A field the reader types into syncs itself, so
//! half-finished edits follow them to another device — which means something
//! has to decide when "typing" has become "an edit". That is this: three
//! seconds of quiet, or a blur, whichever comes first. Validation raised by a
//! sync stays beside the field until a later sync resolves it, rather than
//! disappearing on the next keystroke.
//!
//! Pure, and driven by an explicit clock, so the whole machine is testable
//! without a browser or a three-second wait.

/// The quiet period. Owner ruling: "this should only trigger after the user
/// has defocused, or on a 3s typing debounce".
pub const DEBOUNCE_MS: u64 = 3_000;

/// Where a field is in its cycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Nothing to send.
    Idle,
    /// An edit is waiting out the debounce.
    Waiting {
        /// When it may be sent, in the caller's milliseconds.
        due_ms: u64,
    },
    /// A command is in flight.
    Saving,
}

/// One field's sync state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Debounce {
    phase: Phase,
    note: Option<String>,
    version: u64,
}

impl Default for Debounce {
    fn default() -> Self {
        Self::new()
    }
}

impl Debounce {
    /// A field with nothing to send and nothing to say.
    pub fn new() -> Self {
        Self {
            phase: Phase::Idle,
            note: None,
            version: 0,
        }
    }

    /// Where the field is.
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The validation to show beside the field, if any.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    /// The client's edit version. A commit carries it, so the server can tell
    /// which edit it is being asked to promote.
    pub fn version(&self) -> u64 {
        self.version
    }

    /// The reader typed. Never sends: it restarts the quiet period.
    pub fn edited(&mut self, now_ms: u64) {
        self.version += 1;
        if self.phase != Phase::Saving {
            self.phase = Phase::Waiting {
                due_ms: now_ms + DEBOUNCE_MS,
            };
        }
    }

    /// The field lost focus. Sends whatever is waiting, immediately.
    pub fn blurred(&mut self, _now_ms: u64) -> Option<u64> {
        match self.phase {
            Phase::Waiting { .. } => self.start(),
            _ => None,
        }
    }

    /// The clock moved. Sends if the quiet period is over.
    pub fn elapsed(&mut self, now_ms: u64) -> Option<u64> {
        match self.phase {
            Phase::Waiting { due_ms } if now_ms >= due_ms => self.start(),
            _ => None,
        }
    }

    /// The server took the edit at `version`.
    ///
    /// If the reader kept typing while it was in flight, the field goes
    /// straight back to waiting — with no quiet period, since the reader
    /// already served one.
    pub fn accepted(&mut self, now_ms: u64, version: u64) {
        self.settle(now_ms, version, None);
    }

    /// The server refused the edit at `version`, and why. The reason sits
    /// beside the field until a later sync succeeds.
    pub fn rejected(&mut self, now_ms: u64, version: u64, reason: impl Into<String>) {
        self.settle(now_ms, version, Some(reason.into()));
    }

    fn start(&mut self) -> Option<u64> {
        self.phase = Phase::Saving;
        Some(self.version)
    }

    fn settle(&mut self, now_ms: u64, version: u64, note: Option<String>) {
        if self.phase != Phase::Saving {
            // A reply to a sync this field has already moved past.
            return;
        }
        if note.is_some() {
            self.note = note;
        } else if version == self.version {
            self.note = None;
        }
        self.phase = if version == self.version {
            Phase::Idle
        } else {
            Phase::Waiting { due_ms: now_ms }
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quiet_three_seconds_sends_the_edit() {
        let mut field = Debounce::new();
        field.edited(0);
        assert_eq!(field.phase(), Phase::Waiting { due_ms: 3_000 });
        assert_eq!(field.elapsed(2_999), None);
        assert_eq!(field.elapsed(3_000), Some(1));
        assert_eq!(field.phase(), Phase::Saving);
    }

    #[test]
    fn every_keystroke_restarts_the_quiet_period() {
        let mut field = Debounce::new();
        field.edited(0);
        field.edited(1_500);
        assert_eq!(field.elapsed(3_000), None, "the clock restarted at 1500");
        assert_eq!(field.elapsed(4_500), Some(2));
    }

    #[test]
    fn a_blur_sends_without_waiting() {
        let mut field = Debounce::new();
        field.edited(0);
        assert_eq!(field.blurred(10), Some(1));
        assert_eq!(field.phase(), Phase::Saving);
    }

    #[test]
    fn a_blur_with_nothing_pending_sends_nothing() {
        let mut field = Debounce::new();
        assert_eq!(field.blurred(0), None);
        field.edited(0);
        field.blurred(1);
        assert_eq!(field.blurred(2), None, "already in flight");
    }

    #[test]
    fn typing_during_a_sync_queues_the_next_one() {
        let mut field = Debounce::new();
        field.edited(0);
        let sent = field.blurred(1).expect("sent");
        field.edited(100);
        assert_eq!(field.phase(), Phase::Saving, "one command at a time");

        field.accepted(200, sent);
        assert_eq!(
            field.phase(),
            Phase::Waiting { due_ms: 200 },
            "the newer edit goes immediately"
        );
        assert_eq!(field.elapsed(200), Some(2));
    }

    #[test]
    fn an_accepted_sync_clears_the_field() {
        let mut field = Debounce::new();
        field.edited(0);
        let sent = field.blurred(1).expect("sent");
        field.accepted(2, sent);
        assert_eq!(field.phase(), Phase::Idle);
        assert_eq!(field.note(), None);
    }

    #[test]
    fn validation_stays_until_a_later_sync_resolves_it() {
        let mut field = Debounce::new();
        field.edited(0);
        let sent = field.blurred(1).expect("sent");
        field.rejected(2, sent, "an address needs an @");
        assert_eq!(field.note(), Some("an address needs an @"));
        assert_eq!(field.phase(), Phase::Idle);

        // Typing does not make the complaint go away.
        field.edited(10);
        assert_eq!(field.note(), Some("an address needs an @"));

        let sent = field.blurred(20).expect("sent");
        field.accepted(21, sent);
        assert_eq!(field.note(), None);
    }

    #[test]
    fn a_reply_to_a_sync_the_field_moved_past_is_ignored() {
        let mut field = Debounce::new();
        field.edited(0);
        let stale = field.blurred(1).expect("sent");
        field.accepted(2, stale);
        field.edited(3);

        field.rejected(4, stale, "from the old request");
        assert_eq!(field.note(), None, "the field is not saving");
        assert_eq!(field.phase(), Phase::Waiting { due_ms: 3_003 });
    }

    #[test]
    fn the_version_counts_edits_so_a_commit_can_name_one() {
        let mut field = Debounce::new();
        assert_eq!(field.version(), 0);
        field.edited(0);
        field.edited(1);
        assert_eq!(field.version(), 2);
    }
}
