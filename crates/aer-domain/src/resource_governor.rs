//! Deterministic resource admission and policy-lattice enforcement.

use std::{collections::BTreeMap, error::Error, fmt};

/// Hard-resource vector. Values are conservative upper bounds, never implicit
/// permission to consume resources not represented here.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct ResourceVector {
    pub worker_slots: u64,
    pub cpu_millis: u64,
    pub memory_bytes: u64,
    pub disk_bytes: u64,
    pub pids: u64,
    pub open_files: u64,
    pub wall_time_ms: u64,
    pub network_bytes: u64,
    pub local_ports: u64,
    pub provider_requests: u64,
    pub provider_input_tokens: u64,
    pub provider_output_tokens: u64,
    pub provider_concurrency: u64,
    pub monetary_microusd: u64,
    pub gpu_memory_bytes: u64,
}

impl ResourceVector {
    fn checked_add(self, other: Self) -> Option<Self> {
        Some(Self {
            worker_slots: self.worker_slots.checked_add(other.worker_slots)?,
            cpu_millis: self.cpu_millis.checked_add(other.cpu_millis)?,
            memory_bytes: self.memory_bytes.checked_add(other.memory_bytes)?,
            disk_bytes: self.disk_bytes.checked_add(other.disk_bytes)?,
            pids: self.pids.checked_add(other.pids)?,
            open_files: self.open_files.checked_add(other.open_files)?,
            wall_time_ms: self.wall_time_ms.checked_add(other.wall_time_ms)?,
            network_bytes: self.network_bytes.checked_add(other.network_bytes)?,
            local_ports: self.local_ports.checked_add(other.local_ports)?,
            provider_requests: self
                .provider_requests
                .checked_add(other.provider_requests)?,
            provider_input_tokens: self
                .provider_input_tokens
                .checked_add(other.provider_input_tokens)?,
            provider_output_tokens: self
                .provider_output_tokens
                .checked_add(other.provider_output_tokens)?,
            provider_concurrency: self
                .provider_concurrency
                .checked_add(other.provider_concurrency)?,
            monetary_microusd: self
                .monetary_microusd
                .checked_add(other.monetary_microusd)?,
            gpu_memory_bytes: self.gpu_memory_bytes.checked_add(other.gpu_memory_bytes)?,
        })
    }

    fn checked_sub(self, other: Self) -> Option<Self> {
        Some(Self {
            worker_slots: self.worker_slots.checked_sub(other.worker_slots)?,
            cpu_millis: self.cpu_millis.checked_sub(other.cpu_millis)?,
            memory_bytes: self.memory_bytes.checked_sub(other.memory_bytes)?,
            disk_bytes: self.disk_bytes.checked_sub(other.disk_bytes)?,
            pids: self.pids.checked_sub(other.pids)?,
            open_files: self.open_files.checked_sub(other.open_files)?,
            wall_time_ms: self.wall_time_ms.checked_sub(other.wall_time_ms)?,
            network_bytes: self.network_bytes.checked_sub(other.network_bytes)?,
            local_ports: self.local_ports.checked_sub(other.local_ports)?,
            provider_requests: self
                .provider_requests
                .checked_sub(other.provider_requests)?,
            provider_input_tokens: self
                .provider_input_tokens
                .checked_sub(other.provider_input_tokens)?,
            provider_output_tokens: self
                .provider_output_tokens
                .checked_sub(other.provider_output_tokens)?,
            provider_concurrency: self
                .provider_concurrency
                .checked_sub(other.provider_concurrency)?,
            monetary_microusd: self
                .monetary_microusd
                .checked_sub(other.monetary_microusd)?,
            gpu_memory_bytes: self.gpu_memory_bytes.checked_sub(other.gpu_memory_bytes)?,
        })
    }

    #[must_use]
    pub fn fits_within(self, hard: Self) -> bool {
        self.worker_slots <= hard.worker_slots
            && self.cpu_millis <= hard.cpu_millis
            && self.memory_bytes <= hard.memory_bytes
            && self.disk_bytes <= hard.disk_bytes
            && self.pids <= hard.pids
            && self.open_files <= hard.open_files
            && self.wall_time_ms <= hard.wall_time_ms
            && self.network_bytes <= hard.network_bytes
            && self.local_ports <= hard.local_ports
            && self.provider_requests <= hard.provider_requests
            && self.provider_input_tokens <= hard.provider_input_tokens
            && self.provider_output_tokens <= hard.provider_output_tokens
            && self.provider_concurrency <= hard.provider_concurrency
            && self.monetary_microusd <= hard.monetary_microusd
            && self.gpu_memory_bytes <= hard.gpu_memory_bytes
    }

    #[must_use]
    pub fn component_min(self, other: Self) -> Self {
        Self {
            worker_slots: self.worker_slots.min(other.worker_slots),
            cpu_millis: self.cpu_millis.min(other.cpu_millis),
            memory_bytes: self.memory_bytes.min(other.memory_bytes),
            disk_bytes: self.disk_bytes.min(other.disk_bytes),
            pids: self.pids.min(other.pids),
            open_files: self.open_files.min(other.open_files),
            wall_time_ms: self.wall_time_ms.min(other.wall_time_ms),
            network_bytes: self.network_bytes.min(other.network_bytes),
            local_ports: self.local_ports.min(other.local_ports),
            provider_requests: self.provider_requests.min(other.provider_requests),
            provider_input_tokens: self.provider_input_tokens.min(other.provider_input_tokens),
            provider_output_tokens: self
                .provider_output_tokens
                .min(other.provider_output_tokens),
            provider_concurrency: self.provider_concurrency.min(other.provider_concurrency),
            monetary_microusd: self.monetary_microusd.min(other.monetary_microusd),
            gpu_memory_bytes: self.gpu_memory_bytes.min(other.gpu_memory_bytes),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResourceEstimate {
    Known(ResourceVector),
    UpperBound(ResourceVector),
    Unknown,
}

impl ResourceEstimate {
    fn bound(self) -> Result<ResourceVector, ResourceError> {
        match self {
            Self::Known(vector) | Self::UpperBound(vector) => Ok(vector),
            Self::Unknown => Err(ResourceError::UnknownDemand),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AdmissionClass {
    Generator,
    Verifier,
    Recovery,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ResourceLimits {
    pub hard: ResourceVector,
    pub reserved_verifier_workers: u64,
}

impl ResourceLimits {
    pub fn new(
        hard: ResourceVector,
        reserved_verifier_workers: u64,
    ) -> Result<Self, ResourceError> {
        if reserved_verifier_workers > hard.worker_slots {
            return Err(ResourceError::InvalidVerifierReservation);
        }
        Ok(Self {
            hard,
            reserved_verifier_workers,
        })
    }

    /// Restriction lattice: a lower-authority layer can lower hard caps or
    /// reserve more verifier capacity, but can never widen an upper hard cap.
    #[must_use]
    pub fn restrict_with(self, lower: Self) -> Self {
        let hard = self.hard.component_min(lower.hard);
        let reserved_verifier_workers = self
            .reserved_verifier_workers
            .max(lower.reserved_verifier_workers)
            .min(hard.worker_slots);
        Self {
            hard,
            reserved_verifier_workers,
        }
    }
}

/// Applies organization -> project -> run -> task resource restrictions.
#[must_use]
pub fn resolve_resource_limits(layers: &[ResourceLimits]) -> Option<ResourceLimits> {
    layers.iter().copied().reduce(ResourceLimits::restrict_with)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Reservation {
    pub id: u64,
    pub owner: String,
    pub class: AdmissionClass,
    pub demand: ResourceVector,
}

pub struct ResourceGovernor {
    limits: ResourceLimits,
    used: ResourceVector,
    reservations: BTreeMap<u64, Reservation>,
    owner_index: BTreeMap<String, u64>,
    next_id: u64,
}

impl ResourceGovernor {
    #[must_use]
    pub fn new(limits: ResourceLimits) -> Self {
        Self {
            limits,
            used: ResourceVector::default(),
            reservations: BTreeMap::new(),
            owner_index: BTreeMap::new(),
            next_id: 1,
        }
    }

    pub fn admit(
        &mut self,
        owner: impl Into<String>,
        class: AdmissionClass,
        estimate: ResourceEstimate,
    ) -> Result<Reservation, ResourceError> {
        let owner = owner.into();
        if owner.trim().is_empty() {
            return Err(ResourceError::EmptyOwner);
        }
        if self.owner_index.contains_key(&owner) {
            return Err(ResourceError::OwnerAlreadyReserved(owner));
        }
        let demand = estimate.bound()?;
        let next = self
            .used
            .checked_add(demand)
            .ok_or(ResourceError::ArithmeticOverflow)?;
        if !next.fits_within(self.limits.hard) {
            return Err(ResourceError::HardLimitExceeded);
        }

        if class != AdmissionClass::Verifier {
            let non_verifier_workers = self
                .reservations
                .values()
                .filter(|reservation| reservation.class != AdmissionClass::Verifier)
                .try_fold(0_u64, |total, reservation| {
                    total.checked_add(reservation.demand.worker_slots)
                })
                .ok_or(ResourceError::ArithmeticOverflow)?;
            let generator_ceiling = self
                .limits
                .hard
                .worker_slots
                .saturating_sub(self.limits.reserved_verifier_workers);
            if non_verifier_workers
                .checked_add(demand.worker_slots)
                .ok_or(ResourceError::ArithmeticOverflow)?
                > generator_ceiling
            {
                return Err(ResourceError::VerifierCapacityProtected);
            }
        }

        let id = self.next_id;
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or(ResourceError::ArithmeticOverflow)?;
        let reservation = Reservation {
            id,
            owner: owner.clone(),
            class,
            demand,
        };
        self.used = next;
        self.owner_index.insert(owner, id);
        self.reservations.insert(id, reservation.clone());
        Ok(reservation)
    }

    /// Releases a reservation without mutating any index until all accounting
    /// preconditions have been validated.
    pub fn release(&mut self, reservation_id: u64) -> Result<Reservation, ResourceError> {
        let reservation = self
            .reservations
            .get(&reservation_id)
            .cloned()
            .ok_or(ResourceError::UnknownReservation(reservation_id))?;
        if self.owner_index.get(&reservation.owner).copied() != Some(reservation_id) {
            return Err(ResourceError::AccountingInvariant);
        }
        let next_used = self
            .used
            .checked_sub(reservation.demand)
            .ok_or(ResourceError::AccountingInvariant)?;

        self.reservations.remove(&reservation_id);
        self.owner_index.remove(&reservation.owner);
        self.used = next_used;
        Ok(reservation)
    }

    #[must_use]
    pub const fn usage(&self) -> ResourceVector {
        self.used
    }

    #[must_use]
    pub fn reservation_for(&self, owner: &str) -> Option<&Reservation> {
        self.owner_index
            .get(owner)
            .and_then(|id| self.reservations.get(id))
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ResourceError {
    UnknownDemand,
    EmptyOwner,
    InvalidVerifierReservation,
    OwnerAlreadyReserved(String),
    HardLimitExceeded,
    VerifierCapacityProtected,
    ArithmeticOverflow,
    AccountingInvariant,
    UnknownReservation(u64),
}

impl fmt::Display for ResourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownDemand => formatter.write_str("resource demand is unknown"),
            Self::EmptyOwner => formatter.write_str("resource reservation owner must be non-empty"),
            Self::InvalidVerifierReservation => {
                formatter.write_str("verifier reservation exceeds total worker capacity")
            }
            Self::OwnerAlreadyReserved(owner) => {
                write!(
                    formatter,
                    "owner already has an active reservation: {owner}"
                )
            }
            Self::HardLimitExceeded => formatter.write_str("hard resource limit exceeded"),
            Self::VerifierCapacityProtected => {
                formatter.write_str("admission would consume verifier-reserved worker capacity")
            }
            Self::ArithmeticOverflow => formatter.write_str("resource accounting overflow"),
            Self::AccountingInvariant => {
                formatter.write_str("resource accounting invariant violated")
            }
            Self::UnknownReservation(id) => write!(formatter, "unknown reservation: {id}"),
        }
    }
}

impl Error for ResourceError {}

#[cfg(test)]
mod tests {
    use super::{
        AdmissionClass, ResourceEstimate, ResourceGovernor, ResourceLimits, ResourceVector,
        resolve_resource_limits,
    };

    fn vector(workers: u64, memory: u64) -> ResourceVector {
        ResourceVector {
            worker_slots: workers,
            memory_bytes: memory,
            ..ResourceVector::default()
        }
    }

    #[test]
    fn lower_policy_layer_cannot_widen_org_hard_cap() {
        let org = ResourceLimits::new(vector(4, 100), 1).expect("org limits");
        let project = ResourceLimits::new(vector(64, 1_000), 0).expect("project request");
        let resolved = resolve_resource_limits(&[org, project]).expect("resolved limits");
        assert_eq!(resolved.hard.worker_slots, 4);
        assert_eq!(resolved.hard.memory_bytes, 100);
        assert_eq!(resolved.reserved_verifier_workers, 1);
    }

    #[test]
    fn restriction_lattice_is_monotone_across_small_exhaustive_domain() {
        for upper_workers in 1..=6 {
            for lower_workers in 1..=8 {
                for upper_memory in 1..=6 {
                    for lower_memory in 1..=8 {
                        let upper = ResourceLimits::new(
                            vector(upper_workers, upper_memory),
                            upper_workers.min(2),
                        )
                        .expect("upper");
                        let lower = ResourceLimits::new(
                            vector(lower_workers, lower_memory),
                            lower_workers.min(3),
                        )
                        .expect("lower");
                        let resolved = upper.restrict_with(lower);
                        assert!(resolved.hard.fits_within(upper.hard));
                        assert!(resolved.hard.fits_within(lower.hard));
                        assert!(resolved.reserved_verifier_workers <= resolved.hard.worker_slots);
                    }
                }
            }
        }
    }

    #[test]
    fn verifier_capacity_is_protected_under_generator_load() {
        let limits = ResourceLimits::new(vector(4, 100), 1).expect("limits");
        let mut governor = ResourceGovernor::new(limits);
        for index in 0..3 {
            governor
                .admit(
                    format!("generator-{index}"),
                    AdmissionClass::Generator,
                    ResourceEstimate::Known(vector(1, 10)),
                )
                .expect("generator within non-verifier capacity");
        }
        assert!(
            governor
                .admit(
                    "generator-overflow",
                    AdmissionClass::Generator,
                    ResourceEstimate::Known(vector(1, 10)),
                )
                .is_err()
        );
        governor
            .admit(
                "verifier",
                AdmissionClass::Verifier,
                ResourceEstimate::Known(vector(1, 10)),
            )
            .expect("reserved verifier slot remains usable");
        assert_eq!(governor.usage().worker_slots, 4);
    }

    #[test]
    fn admission_never_crosses_worker_hard_cap_or_verifier_reserve() {
        for capacity in 1..=8 {
            for reserve in 0..=capacity {
                let limits = ResourceLimits::new(vector(capacity, 1_000), reserve).expect("limits");
                let mut governor = ResourceGovernor::new(limits);
                let generator_capacity = capacity - reserve;
                for index in 0..generator_capacity {
                    governor
                        .admit(
                            format!("generator-{index}"),
                            AdmissionClass::Generator,
                            ResourceEstimate::Known(vector(1, 1)),
                        )
                        .expect("generator within ceiling");
                }
                assert!(governor.usage().worker_slots <= generator_capacity);
                if reserve > 0 {
                    assert!(
                        governor
                            .admit(
                                "extra-generator",
                                AdmissionClass::Generator,
                                ResourceEstimate::Known(vector(1, 1)),
                            )
                            .is_err()
                    );
                }
                for index in 0..reserve {
                    governor
                        .admit(
                            format!("verifier-{index}"),
                            AdmissionClass::Verifier,
                            ResourceEstimate::Known(vector(1, 1)),
                        )
                        .expect("verifier reserve remains usable");
                }
                assert_eq!(governor.usage().worker_slots, capacity);
            }
        }
    }

    #[test]
    fn release_restores_capacity_and_owner_index() {
        let limits = ResourceLimits::new(vector(2, 100), 0).expect("limits");
        let mut governor = ResourceGovernor::new(limits);
        let reservation = governor
            .admit(
                "task",
                AdmissionClass::Generator,
                ResourceEstimate::Known(vector(1, 10)),
            )
            .expect("reservation");
        governor.release(reservation.id).expect("release");
        assert_eq!(governor.usage(), ResourceVector::default());
        assert!(governor.reservation_for("task").is_none());
        governor
            .admit(
                "task",
                AdmissionClass::Generator,
                ResourceEstimate::Known(vector(1, 10)),
            )
            .expect("owner can reserve again after release");
    }

    #[test]
    fn unknown_demand_fails_closed() {
        let limits = ResourceLimits::new(vector(1, 10), 0).expect("limits");
        let mut governor = ResourceGovernor::new(limits);
        assert!(
            governor
                .admit(
                    "unknown",
                    AdmissionClass::Generator,
                    ResourceEstimate::Unknown,
                )
                .is_err()
        );
    }
}
