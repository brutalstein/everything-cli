//! Cooperative cancellation protocol state.

use std::{error::Error, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationPhase {
    None,
    Requested,
    Draining,
    ForceRequired,
    Completed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CancellationProtocol {
    phase: CancellationPhase,
    requested_at_ms: Option<u64>,
    cleanup_deadline_ms: Option<u64>,
}

impl Default for CancellationProtocol {
    fn default() -> Self {
        Self {
            phase: CancellationPhase::None,
            requested_at_ms: None,
            cleanup_deadline_ms: None,
        }
    }
}

impl CancellationProtocol {
    pub fn request(&mut self, now_ms: u64, cleanup_grace_ms: u64) -> Result<(), CancellationError> {
        if self.phase != CancellationPhase::None {
            return Err(CancellationError::AlreadyRequested);
        }
        self.requested_at_ms = Some(now_ms);
        self.cleanup_deadline_ms = Some(
            now_ms
                .checked_add(cleanup_grace_ms)
                .ok_or(CancellationError::ArithmeticOverflow)?,
        );
        self.phase = CancellationPhase::Requested;
        Ok(())
    }

    pub fn begin_draining(&mut self) -> Result<(), CancellationError> {
        if self.phase != CancellationPhase::Requested {
            return Err(CancellationError::InvalidPhase);
        }
        self.phase = CancellationPhase::Draining;
        Ok(())
    }

    pub fn poll_deadline(&mut self, now_ms: u64) -> Result<CancellationPhase, CancellationError> {
        match self.phase {
            CancellationPhase::Requested | CancellationPhase::Draining => {
                let deadline = self
                    .cleanup_deadline_ms
                    .ok_or(CancellationError::InvalidPhase)?;
                if now_ms >= deadline {
                    self.phase = CancellationPhase::ForceRequired;
                }
                Ok(self.phase)
            }
            CancellationPhase::ForceRequired | CancellationPhase::Completed => Ok(self.phase),
            CancellationPhase::None => Err(CancellationError::InvalidPhase),
        }
    }

    pub fn complete(&mut self) -> Result<(), CancellationError> {
        if !matches!(
            self.phase,
            CancellationPhase::Requested
                | CancellationPhase::Draining
                | CancellationPhase::ForceRequired
        ) {
            return Err(CancellationError::InvalidPhase);
        }
        self.phase = CancellationPhase::Completed;
        Ok(())
    }

    #[must_use]
    pub const fn phase(self) -> CancellationPhase {
        self.phase
    }

    #[must_use]
    pub const fn allows_new_child_actions(self) -> bool {
        matches!(self.phase, CancellationPhase::None)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CancellationError {
    AlreadyRequested,
    InvalidPhase,
    ArithmeticOverflow,
}

impl fmt::Display for CancellationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::AlreadyRequested => formatter.write_str("cancellation already requested"),
            Self::InvalidPhase => formatter.write_str("invalid cancellation phase transition"),
            Self::ArithmeticOverflow => formatter.write_str("cancellation deadline overflow"),
        }
    }
}

impl Error for CancellationError {}

#[cfg(test)]
mod tests {
    use super::{CancellationPhase, CancellationProtocol};

    #[test]
    fn cancellation_stops_child_admission_and_reaches_force_deadline() {
        let mut cancellation = CancellationProtocol::default();
        cancellation.request(10, 5).expect("request");
        assert!(!cancellation.allows_new_child_actions());
        cancellation.begin_draining().expect("drain");
        assert_eq!(
            cancellation.poll_deadline(15).expect("deadline"),
            CancellationPhase::ForceRequired
        );
        cancellation.complete().expect("complete");
        assert_eq!(cancellation.phase(), CancellationPhase::Completed);
    }
}
