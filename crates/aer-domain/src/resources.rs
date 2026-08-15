//! Resource-bound primitives used to make unbounded queues unrepresentable in
//! foundation code.

use std::{error::Error, fmt, num::NonZeroUsize};

/// Explicit finite queue capacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord)]
pub struct QueueCapacity(NonZeroUsize);

impl QueueCapacity {
    /// Creates a finite capacity. Zero is rejected rather than treated as a
    /// sentinel for an unbounded channel.
    pub fn new(value: usize) -> Result<Self, QueuePolicyError> {
        NonZeroUsize::new(value)
            .map(Self)
            .ok_or(QueuePolicyError::ZeroCapacity)
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0.get()
    }
}

/// Authority class of data traveling through a queue.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueuePurpose {
    /// Durable/material state that may never be silently dropped or coalesced.
    Authoritative,
    /// Non-authoritative UI/presentation deltas that can be recomputed.
    Presentation,
}

/// Allowed behavior when a bounded queue reaches capacity.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OverflowPolicy {
    /// Apply producer backpressure until capacity is available.
    Backpressure,
    /// Coalesce replaceable presentation updates into the latest value.
    CoalesceLatest,
}

/// Validated queue policy. There is deliberately no `Unbounded` or `Drop`
/// variant.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundedQueuePolicy {
    pub purpose: QueuePurpose,
    pub capacity: QueueCapacity,
    pub overflow: OverflowPolicy,
}

impl BoundedQueuePolicy {
    pub fn new(
        purpose: QueuePurpose,
        capacity: usize,
        overflow: OverflowPolicy,
    ) -> Result<Self, QueuePolicyError> {
        if purpose == QueuePurpose::Authoritative && overflow == OverflowPolicy::CoalesceLatest {
            return Err(QueuePolicyError::AuthoritativeDataCannotBeCoalesced);
        }

        Ok(Self {
            purpose,
            capacity: QueueCapacity::new(capacity)?,
            overflow,
        })
    }
}

/// Invalid resource-policy construction.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum QueuePolicyError {
    ZeroCapacity,
    AuthoritativeDataCannotBeCoalesced,
}

impl fmt::Display for QueuePolicyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroCapacity => {
                formatter.write_str("queue capacity must be finite and greater than zero")
            }
            Self::AuthoritativeDataCannotBeCoalesced => {
                formatter.write_str("authoritative queue data cannot use coalescing overflow")
            }
        }
    }
}

impl Error for QueuePolicyError {}

#[cfg(test)]
mod tests {
    use super::{BoundedQueuePolicy, OverflowPolicy, QueuePolicyError, QueuePurpose};

    #[test]
    fn zero_capacity_is_rejected() {
        let result =
            BoundedQueuePolicy::new(QueuePurpose::Authoritative, 0, OverflowPolicy::Backpressure);

        assert_eq!(result, Err(QueuePolicyError::ZeroCapacity));
    }

    #[test]
    fn authoritative_data_cannot_be_coalesced() {
        let result = BoundedQueuePolicy::new(
            QueuePurpose::Authoritative,
            32,
            OverflowPolicy::CoalesceLatest,
        );

        assert_eq!(
            result,
            Err(QueuePolicyError::AuthoritativeDataCannotBeCoalesced)
        );
    }

    #[test]
    fn presentation_updates_may_be_coalesced_with_finite_capacity() {
        let policy = BoundedQueuePolicy::new(
            QueuePurpose::Presentation,
            8,
            OverflowPolicy::CoalesceLatest,
        )
        .expect("finite presentation policy should be valid");

        assert_eq!(policy.capacity.get(), 8);
    }
}
