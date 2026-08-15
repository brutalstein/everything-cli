//! Bounded in-memory queue implementing the foundation queue policy.

use std::collections::VecDeque;

use crate::resources::{BoundedQueuePolicy, OverflowPolicy};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct QueueTelemetry {
    pub enqueued: u64,
    pub backpressured: u64,
    pub coalesced: u64,
    pub high_watermark: usize,
}

#[derive(Debug, Eq, PartialEq)]
pub enum PushOutcome<T> {
    Enqueued,
    Backpressured(T),
    Coalesced { replaced: T },
}

pub struct BoundedQueue<T> {
    policy: BoundedQueuePolicy,
    items: VecDeque<T>,
    telemetry: QueueTelemetry,
}

impl<T> BoundedQueue<T> {
    #[must_use]
    pub fn new(policy: BoundedQueuePolicy) -> Self {
        Self {
            policy,
            items: VecDeque::with_capacity(policy.capacity.get()),
            telemetry: QueueTelemetry::default(),
        }
    }

    pub fn push(&mut self, item: T) -> PushOutcome<T> {
        if self.items.len() < self.policy.capacity.get() {
            self.items.push_back(item);
            self.telemetry.enqueued = self.telemetry.enqueued.saturating_add(1);
            self.telemetry.high_watermark = self.telemetry.high_watermark.max(self.items.len());
            return PushOutcome::Enqueued;
        }

        match self.policy.overflow {
            OverflowPolicy::Backpressure => {
                self.telemetry.backpressured = self.telemetry.backpressured.saturating_add(1);
                PushOutcome::Backpressured(item)
            }
            OverflowPolicy::CoalesceLatest => {
                let replaced = self
                    .items
                    .pop_back()
                    .expect("positive queue capacity means a full queue has an item");
                self.items.push_back(item);
                self.telemetry.coalesced = self.telemetry.coalesced.saturating_add(1);
                PushOutcome::Coalesced { replaced }
            }
        }
    }

    pub fn pop(&mut self) -> Option<T> {
        self.items.pop_front()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.items.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    #[must_use]
    pub const fn telemetry(&self) -> QueueTelemetry {
        self.telemetry
    }
}

#[cfg(test)]
mod tests {
    use crate::resources::{BoundedQueuePolicy, OverflowPolicy, QueuePurpose};

    use super::{BoundedQueue, PushOutcome};

    #[test]
    fn authoritative_queue_backpressures_without_dropping() {
        let policy = BoundedQueuePolicy::new(
            QueuePurpose::Authoritative,
            1,
            OverflowPolicy::Backpressure,
        )
        .expect("policy");
        let mut queue = BoundedQueue::new(policy);
        assert_eq!(queue.push("first"), PushOutcome::Enqueued);
        assert_eq!(queue.push("second"), PushOutcome::Backpressured("second"));
        assert_eq!(queue.pop(), Some("first"));
        assert_eq!(queue.telemetry().backpressured, 1);
    }

    #[test]
    fn presentation_queue_coalesces_only_at_finite_capacity() {
        let policy = BoundedQueuePolicy::new(
            QueuePurpose::Presentation,
            1,
            OverflowPolicy::CoalesceLatest,
        )
        .expect("policy");
        let mut queue = BoundedQueue::new(policy);
        queue.push(1);
        assert_eq!(queue.push(2), PushOutcome::Coalesced { replaced: 1 });
        assert_eq!(queue.pop(), Some(2));
        assert_eq!(queue.telemetry().coalesced, 1);
    }
}
