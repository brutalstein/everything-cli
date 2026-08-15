//! Deterministic project/run/task lifecycle rules.

use std::{error::Error, fmt};

/// Runtime-only project admission state. This deliberately does not define a
/// durable project wire contract; it controls whether new work may be admitted.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProjectRuntimeState {
    Active,
    Draining,
    Paused,
}

impl ProjectRuntimeState {
    pub fn transition(self, next: Self) -> Result<Self, TransitionError> {
        let valid = matches!(
            (self, next),
            (Self::Active, Self::Draining)
                | (Self::Draining, Self::Paused)
                | (Self::Paused, Self::Active)
        );
        valid
            .then_some(next)
            .ok_or(TransitionError::InvalidProjectTransition {
                from: self,
                to: next,
            })
    }

    #[must_use]
    pub const fn admits_new_work(self) -> bool {
        matches!(self, Self::Active)
    }
}

/// Run states match `docs/schemas/run.schema.json`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunState {
    Pending,
    Interviewing,
    Planning,
    Executing,
    WaitingForUser,
    WaitingForPermission,
    Verifying,
    Recovering,
    Completed,
    Failed,
    Cancelled,
}

impl RunState {
    pub fn transition(self, next: Self) -> Result<Self, TransitionError> {
        use RunState as S;
        let valid = match self {
            S::Pending => matches!(
                next,
                S::Interviewing
                    | S::Planning
                    | S::Executing
                    | S::WaitingForUser
                    | S::WaitingForPermission
                    | S::Failed
                    | S::Cancelled
            ),
            S::Interviewing => matches!(
                next,
                S::Planning
                    | S::WaitingForUser
                    | S::WaitingForPermission
                    | S::Failed
                    | S::Cancelled
            ),
            S::Planning => matches!(
                next,
                S::Executing
                    | S::WaitingForUser
                    | S::WaitingForPermission
                    | S::Recovering
                    | S::Failed
                    | S::Cancelled
            ),
            S::Executing => matches!(
                next,
                S::WaitingForUser
                    | S::WaitingForPermission
                    | S::Verifying
                    | S::Recovering
                    | S::Failed
                    | S::Cancelled
            ),
            S::WaitingForUser | S::WaitingForPermission => matches!(
                next,
                S::Planning
                    | S::Executing
                    | S::Verifying
                    | S::Recovering
                    | S::Failed
                    | S::Cancelled
            ),
            S::Verifying => matches!(
                next,
                S::Completed | S::Recovering | S::Failed | S::Cancelled
            ),
            S::Recovering => matches!(
                next,
                S::Planning
                    | S::Executing
                    | S::WaitingForUser
                    | S::WaitingForPermission
                    | S::Verifying
                    | S::Failed
                    | S::Cancelled
            ),
            S::Completed | S::Failed | S::Cancelled => false,
        };
        valid
            .then_some(next)
            .ok_or(TransitionError::InvalidRunTransition {
                from: self,
                to: next,
            })
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Failed | Self::Cancelled)
    }
}

/// Task states match `docs/schemas/task.schema.json` and the lifecycle in doc 10.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskState {
    Pending,
    Ready,
    Running,
    Blocked,
    Verifying,
    Accepted,
    Rejected,
    Stale,
    Cancelled,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TaskTransitionContext {
    pub proof_accepted: bool,
    pub cancellation_finalized: bool,
}

impl TaskState {
    pub fn transition(
        self,
        next: Self,
        context: TaskTransitionContext,
    ) -> Result<Self, TransitionError> {
        use TaskState as S;
        if next == S::Accepted && !context.proof_accepted {
            return Err(TransitionError::AcceptedRequiresProof);
        }
        if next == S::Cancelled && !context.cancellation_finalized {
            return Err(TransitionError::CancelledRequiresFinalization);
        }

        let valid = match self {
            S::Pending => matches!(next, S::Ready | S::Stale | S::Cancelled),
            S::Ready => matches!(next, S::Running | S::Stale | S::Cancelled),
            S::Running => matches!(next, S::Blocked | S::Verifying | S::Stale | S::Cancelled),
            S::Blocked => matches!(next, S::Ready | S::Stale | S::Cancelled),
            S::Verifying => matches!(next, S::Accepted | S::Rejected | S::Stale | S::Cancelled),
            S::Rejected => matches!(next, S::Ready | S::Stale | S::Cancelled),
            S::Stale => matches!(next, S::Pending | S::Cancelled),
            S::Accepted | S::Cancelled => false,
        };
        valid
            .then_some(next)
            .ok_or(TransitionError::InvalidTaskTransition {
                from: self,
                to: next,
            })
    }

    #[must_use]
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Accepted | Self::Cancelled)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TransitionError {
    InvalidProjectTransition {
        from: ProjectRuntimeState,
        to: ProjectRuntimeState,
    },
    InvalidRunTransition {
        from: RunState,
        to: RunState,
    },
    InvalidTaskTransition {
        from: TaskState,
        to: TaskState,
    },
    AcceptedRequiresProof,
    CancelledRequiresFinalization,
}

impl fmt::Display for TransitionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidProjectTransition { from, to } => {
                write!(
                    formatter,
                    "invalid project runtime transition: {from:?} -> {to:?}"
                )
            }
            Self::InvalidRunTransition { from, to } => {
                write!(formatter, "invalid run transition: {from:?} -> {to:?}")
            }
            Self::InvalidTaskTransition { from, to } => {
                write!(formatter, "invalid task transition: {from:?} -> {to:?}")
            }
            Self::AcceptedRequiresProof => {
                formatter.write_str("accepted task state requires accepted proof")
            }
            Self::CancelledRequiresFinalization => {
                formatter.write_str("cancelled task state requires finalized cancellation")
            }
        }
    }
}

impl Error for TransitionError {}

#[cfg(test)]
mod tests {
    use super::{RunState, TaskState, TaskTransitionContext, TransitionError};

    #[test]
    fn accepted_requires_proof() {
        assert_eq!(
            TaskState::Verifying.transition(TaskState::Accepted, TaskTransitionContext::default()),
            Err(TransitionError::AcceptedRequiresProof)
        );
        assert_eq!(
            TaskState::Verifying.transition(
                TaskState::Accepted,
                TaskTransitionContext {
                    proof_accepted: true,
                    cancellation_finalized: false,
                },
            ),
            Ok(TaskState::Accepted)
        );
    }

    #[test]
    fn terminal_run_cannot_restart() {
        assert!(RunState::Completed.transition(RunState::Executing).is_err());
    }
}
