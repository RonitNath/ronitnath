//! The per-connection bound, and what happens when a client falls behind it.
//!
//! A subscription that cannot keep up must not be allowed to cost the server
//! memory. The queue is therefore fixed: when one wake-up produces more
//! messages than a connection may hold, the backlog is *dropped whole* and
//! replaced by a single [`SubMessage::Resync`]. That is the honest answer —
//! the server can no longer express the client's position as a diff, which is
//! exactly what `resync` means (`rn_api::sub`) — and it is bounded work, which
//! sending the backlog anyway would not be.
//!
//! Dropping is safe because a resync is a full refetch: no row is lost, only
//! the cheap way of learning about it.

use std::collections::VecDeque;

use rn_api::SubMessage;

/// How many messages one connection may hold between writes.
pub const CAPACITY: usize = 256;

/// A bounded queue of outgoing messages.
#[derive(Debug)]
pub struct Outbox {
    queue: VecDeque<SubMessage>,
    capacity: usize,
    overflowed: bool,
}

impl Default for Outbox {
    fn default() -> Self {
        Self::new(CAPACITY)
    }
}

impl Outbox {
    /// A queue holding at most `capacity` messages.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            queue: VecDeque::new(),
            capacity: capacity.max(1),
            overflowed: false,
        }
    }

    /// Enqueue a message, or overflow.
    ///
    /// Once overflowed the queue stays overflowed until it is drained: every
    /// further message is dropped rather than queued, because they are all
    /// about to be superseded by the resync.
    pub fn push(&mut self, message: SubMessage) {
        if self.overflowed {
            return;
        }
        if self.queue.len() >= self.capacity {
            self.queue.clear();
            self.overflowed = true;
            return;
        }
        self.queue.push_back(message);
    }

    /// Take everything to write, resetting the queue.
    pub fn drain(&mut self) -> Vec<SubMessage> {
        if self.overflowed {
            self.overflowed = false;
            self.queue.clear();
            return vec![SubMessage::Resync];
        }
        self.queue.drain(..).collect()
    }

    /// Whether anything is waiting.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queue.is_empty() && !self.overflowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diff(offset: u64) -> SubMessage {
        SubMessage::Diff {
            offset,
            query: "sessions".into(),
            ops: Vec::new(),
        }
    }

    #[test]
    fn messages_come_out_in_the_order_they_went_in() {
        let mut outbox = Outbox::new(4);
        assert!(outbox.is_empty());
        outbox.push(diff(1));
        outbox.push(diff(2));
        assert!(!outbox.is_empty());
        assert_eq!(outbox.drain(), vec![diff(1), diff(2)]);
        assert!(outbox.is_empty());
    }

    #[test]
    fn overflowing_replaces_the_whole_backlog_with_one_resync() {
        let mut outbox = Outbox::new(2);
        outbox.push(diff(1));
        outbox.push(diff(2));
        outbox.push(diff(3));
        assert_eq!(outbox.drain(), vec![SubMessage::Resync]);
    }

    #[test]
    fn nothing_queued_after_an_overflow_survives_it() {
        let mut outbox = Outbox::new(1);
        outbox.push(diff(1));
        outbox.push(diff(2));
        outbox.push(diff(3));
        outbox.push(diff(4));
        assert_eq!(outbox.drain(), vec![SubMessage::Resync]);
        // And the queue is usable again straight afterwards.
        outbox.push(diff(5));
        assert_eq!(outbox.drain(), vec![diff(5)]);
    }

    #[test]
    fn a_zero_capacity_queue_still_holds_one_message_rather_than_spinning() {
        let mut outbox = Outbox::new(0);
        outbox.push(diff(1));
        assert_eq!(outbox.drain(), vec![diff(1)]);
    }
}
