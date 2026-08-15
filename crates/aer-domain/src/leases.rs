//! Single-owner task leases with explicit heartbeat, expiry, and reconciliation.

use std::{collections::BTreeMap, error::Error, fmt};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectClass {
    Pure,
    WorkspaceLocal,
    ExternalMutating,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LeaseHealth {
    Healthy,
    Suspect,
    Expired,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LeasePolicy {
    suspect_after_ms: u64,
    expire_after_ms: u64,
}

impl LeasePolicy {
    pub fn new(suspect_after_ms: u64, expire_after_ms: u64) -> Result<Self, LeaseError> {
        if suspect_after_ms == 0 || expire_after_ms <= suspect_after_ms {
            return Err(LeaseError::InvalidPolicy);
        }
        Ok(Self {
            suspect_after_ms,
            expire_after_ms,
        })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Lease {
    pub id: u64,
    pub task_id: String,
    pub owner: String,
    pub effect_class: EffectClass,
    pub last_heartbeat_ms: u64,
    pub suspect_at_ms: u64,
    pub expires_at_ms: u64,
    pub expiry_observed: bool,
}

impl Lease {
    #[must_use]
    pub const fn health(&self, now_ms: u64) -> LeaseHealth {
        if now_ms >= self.expires_at_ms {
            LeaseHealth::Expired
        } else if now_ms >= self.suspect_at_ms {
            LeaseHealth::Suspect
        } else {
            LeaseHealth::Healthy
        }
    }
}

pub struct LeaseBook {
    policy: LeasePolicy,
    leases: BTreeMap<String, Lease>,
    next_id: u64,
}

impl LeaseBook {
    #[must_use]
    pub fn new(policy: LeasePolicy) -> Self {
        Self {
            policy,
            leases: BTreeMap::new(),
            next_id: 1,
        }
    }

    pub fn acquire(
        &mut self,
        task_id: impl Into<String>,
        owner: impl Into<String>,
        effect_class: EffectClass,
        now_ms: u64,
    ) -> Result<Lease, LeaseError> {
        let task_id = task_id.into();
        let owner = owner.into();
        if task_id.trim().is_empty() || owner.trim().is_empty() {
            return Err(LeaseError::EmptyIdentity);
        }
        if let Some(existing) = self.leases.get(&task_id) {
            return if existing.health(now_ms) == LeaseHealth::Expired {
                Err(LeaseError::ReconciliationRequired(task_id))
            } else {
                Err(LeaseError::AlreadyActive(task_id))
            };
        }

        let suspect_at_ms = now_ms
            .checked_add(self.policy.suspect_after_ms)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let expires_at_ms = now_ms
            .checked_add(self.policy.expire_after_ms)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let lease = Lease {
            id,
            task_id: task_id.clone(),
            owner,
            effect_class,
            last_heartbeat_ms: now_ms,
            suspect_at_ms,
            expires_at_ms,
            expiry_observed: false,
        };
        self.leases.insert(task_id, lease.clone());
        Ok(lease)
    }

    pub fn heartbeat(
        &mut self,
        task_id: &str,
        lease_id: u64,
        now_ms: u64,
    ) -> Result<Lease, LeaseError> {
        let lease = self
            .leases
            .get_mut(task_id)
            .ok_or_else(|| LeaseError::UnknownTask(task_id.to_owned()))?;
        if lease.id != lease_id {
            return Err(LeaseError::LeaseMismatch);
        }
        if lease.health(now_ms) == LeaseHealth::Expired {
            return Err(LeaseError::ReconciliationRequired(task_id.to_owned()));
        }

        let suspect_at_ms = now_ms
            .checked_add(self.policy.suspect_after_ms)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        let expires_at_ms = now_ms
            .checked_add(self.policy.expire_after_ms)
            .ok_or(LeaseError::ArithmeticOverflow)?;
        lease.last_heartbeat_ms = now_ms;
        lease.suspect_at_ms = suspect_at_ms;
        lease.expires_at_ms = expires_at_ms;
        Ok(lease.clone())
    }

    pub fn observe_expiry(
        &mut self,
        task_id: &str,
        lease_id: u64,
        now_ms: u64,
    ) -> Result<Lease, LeaseError> {
        let lease = self
            .leases
            .get_mut(task_id)
            .ok_or_else(|| LeaseError::UnknownTask(task_id.to_owned()))?;
        if lease.id != lease_id {
            return Err(LeaseError::LeaseMismatch);
        }
        if lease.health(now_ms) != LeaseHealth::Expired {
            return Err(LeaseError::NotExpired);
        }
        lease.expiry_observed = true;
        Ok(lease.clone())
    }

    pub fn reconcile_expired(
        &mut self,
        task_id: &str,
        lease_id: u64,
        now_ms: u64,
    ) -> Result<Lease, LeaseError> {
        let lease = self
            .leases
            .get(task_id)
            .ok_or_else(|| LeaseError::UnknownTask(task_id.to_owned()))?;
        if lease.id != lease_id {
            return Err(LeaseError::LeaseMismatch);
        }
        if lease.health(now_ms) != LeaseHealth::Expired || !lease.expiry_observed {
            return Err(LeaseError::ReconciliationRequired(task_id.to_owned()));
        }
        self.leases
            .remove(task_id)
            .ok_or_else(|| LeaseError::UnknownTask(task_id.to_owned()))
    }

    pub fn release(&mut self, task_id: &str, lease_id: u64) -> Result<Lease, LeaseError> {
        let lease = self
            .leases
            .get(task_id)
            .ok_or_else(|| LeaseError::UnknownTask(task_id.to_owned()))?;
        if lease.id != lease_id {
            return Err(LeaseError::LeaseMismatch);
        }
        self.leases
            .remove(task_id)
            .ok_or_else(|| LeaseError::UnknownTask(task_id.to_owned()))
    }

    #[must_use]
    pub fn lease(&self, task_id: &str) -> Option<&Lease> {
        self.leases.get(task_id)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LeaseError {
    InvalidPolicy,
    EmptyIdentity,
    AlreadyActive(String),
    ReconciliationRequired(String),
    UnknownTask(String),
    LeaseMismatch,
    NotExpired,
    ArithmeticOverflow,
}

impl fmt::Display for LeaseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidPolicy => formatter.write_str("lease policy requires 0 < suspect < expiry"),
            Self::EmptyIdentity => formatter.write_str("lease task/owner identity must be non-empty"),
            Self::AlreadyActive(task) => write!(formatter, "task already has an active lease: {task}"),
            Self::ReconciliationRequired(task) => {
                write!(formatter, "expired lease requires reconciliation before retry: {task}")
            }
            Self::UnknownTask(task) => write!(formatter, "task has no lease: {task}"),
            Self::LeaseMismatch => formatter.write_str("lease identifier does not match active lease"),
            Self::NotExpired => formatter.write_str("lease has not expired"),
            Self::ArithmeticOverflow => formatter.write_str("lease deadline arithmetic overflow"),
        }
    }
}

impl Error for LeaseError {}

#[cfg(test)]
mod tests {
    use super::{EffectClass, LeaseBook, LeaseError, LeaseHealth, LeasePolicy};

    #[test]
    fn one_active_lease_and_expired_lease_requires_reconciliation() {
        let policy = LeasePolicy::new(10, 20).expect("policy");
        let mut leases = LeaseBook::new(policy);
        let first = leases
            .acquire("task", "worker-1", EffectClass::ExternalMutating, 0)
            .expect("first lease");
        assert_eq!(
            leases.acquire("task", "worker-2", EffectClass::ExternalMutating, 1),
            Err(LeaseError::AlreadyActive("task".to_owned()))
        );
        assert_eq!(first.health(10), LeaseHealth::Suspect);
        assert_eq!(
            leases.acquire("task", "worker-2", EffectClass::ExternalMutating, 20),
            Err(LeaseError::ReconciliationRequired("task".to_owned()))
        );
        leases
            .observe_expiry("task", first.id, 20)
            .expect("observe expiry");
        leases
            .reconcile_expired("task", first.id, 20)
            .expect("reconcile");
        leases
            .acquire("task", "worker-2", EffectClass::ExternalMutating, 21)
            .expect("retry only after reconciliation");
    }

    #[test]
    fn heartbeat_overflow_does_not_partially_mutate_lease() {
        let policy = LeasePolicy::new(10, 20).expect("policy");
        let mut leases = LeaseBook::new(policy);
        let first = leases
            .acquire(
                "task",
                "worker",
                EffectClass::Pure,
                u64::MAX - 30,
            )
            .expect("lease");
        assert_eq!(
            leases.heartbeat("task", first.id, u64::MAX - 15),
            Err(LeaseError::ArithmeticOverflow)
        );
        assert_eq!(leases.lease("task"), Some(&first));
    }
}
