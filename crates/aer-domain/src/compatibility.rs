//! Independent compatibility/version surfaces.
//!
//! AER deliberately does not collapse durable and wire compatibility into the
//! package version. Reserved surfaces become active only when their subsystem
//! actually exists.

/// Initial durable SQLite schema version owned by `aer-storage`.
pub const DATABASE_SCHEMA_VERSION: u32 = 1;
/// Initial append-only event schema version.
pub const EVENT_SCHEMA_VERSION: u32 = 1;

/// Lifecycle of a compatibility surface in the current implementation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SurfaceLifecycle {
    Reserved,
    Active { version: u32 },
}

/// One independently versioned compatibility surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CompatibilitySurface {
    RuntimeApi,
    DatabaseSchema,
    EventSchema,
    EngineeringIrSchema,
    ToolAbi,
    HandoffAbi,
    ConfigurationSchema,
    PolicySchema,
    Sdk,
    DomainProfile,
}

/// Stable metadata for a compatibility surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompatibilityDescriptor {
    pub surface: CompatibilitySurface,
    pub key: &'static str,
    pub lifecycle: SurfaceLifecycle,
}

impl CompatibilitySurface {
    /// Returns the current implementation status without inventing versions for
    /// subsystems that have not been implemented yet.
    #[must_use]
    pub const fn descriptor(self) -> CompatibilityDescriptor {
        match self {
            Self::RuntimeApi => reserved(self, "runtime_api_version"),
            Self::DatabaseSchema => {
                active(self, "database_schema_version", DATABASE_SCHEMA_VERSION)
            }
            Self::EventSchema => active(self, "event_schema_version", EVENT_SCHEMA_VERSION),
            Self::EngineeringIrSchema => active(self, "engineering_ir_schema_version", 1),
            Self::ToolAbi => reserved(self, "tool_abi_version"),
            Self::HandoffAbi => active(self, "handoff_abi_version", 1),
            Self::ConfigurationSchema => active(self, "config_schema_version", 1),
            Self::PolicySchema => active(self, "policy_schema_version", 1),
            Self::Sdk => reserved(self, "sdk_version"),
            Self::DomainProfile => reserved(self, "domain_profile_version"),
        }
    }
}

const fn reserved(surface: CompatibilitySurface, key: &'static str) -> CompatibilityDescriptor {
    CompatibilityDescriptor {
        surface,
        key,
        lifecycle: SurfaceLifecycle::Reserved,
    }
}

const fn active(
    surface: CompatibilitySurface,
    key: &'static str,
    version: u32,
) -> CompatibilityDescriptor {
    CompatibilityDescriptor {
        surface,
        key,
        lifecycle: SurfaceLifecycle::Active { version },
    }
}

/// Package/binary version is intentionally distinct from all compatibility surfaces.
pub const AER_BINARY_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Complete independent-version axis registry from the architecture baseline.
pub const COMPATIBILITY_SURFACES: [CompatibilitySurface; 10] = [
    CompatibilitySurface::RuntimeApi,
    CompatibilitySurface::DatabaseSchema,
    CompatibilitySurface::EventSchema,
    CompatibilitySurface::EngineeringIrSchema,
    CompatibilitySurface::ToolAbi,
    CompatibilitySurface::HandoffAbi,
    CompatibilitySurface::ConfigurationSchema,
    CompatibilitySurface::PolicySchema,
    CompatibilitySurface::Sdk,
    CompatibilitySurface::DomainProfile,
];

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{
        COMPATIBILITY_SURFACES, CompatibilitySurface, DATABASE_SCHEMA_VERSION,
        EVENT_SCHEMA_VERSION, SurfaceLifecycle,
    };

    #[test]
    fn compatibility_keys_are_unique_and_active_versions_are_nonzero() {
        let mut keys = BTreeSet::new();

        for surface in COMPATIBILITY_SURFACES {
            let descriptor = surface.descriptor();
            assert!(keys.insert(descriptor.key));
            if let SurfaceLifecycle::Active { version } = descriptor.lifecycle {
                assert!(version > 0);
            }
        }
    }

    #[test]
    fn durable_storage_surfaces_are_active_at_v1() {
        assert_eq!(DATABASE_SCHEMA_VERSION, 1);
        assert_eq!(EVENT_SCHEMA_VERSION, 1);
        assert_eq!(
            CompatibilitySurface::DatabaseSchema.descriptor().lifecycle,
            SurfaceLifecycle::Active { version: 1 }
        );
        assert_eq!(
            CompatibilitySurface::EventSchema.descriptor().lifecycle,
            SurfaceLifecycle::Active { version: 1 }
        );
    }
}
