//! Compile-time platform capability baseline for execution evidence.
//!
//! These flags describe what the current everything runtime can truthfully
//! provide on this build target. They are intentionally conservative: a
//! platform primitive existing in theory does not imply that a stronger sandbox
//! backend is implemented.

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PlatformCapabilities {
    pub os: String,
    pub architecture: String,
    pub native_file_locking: bool,
    pub direct_child_termination: bool,
    pub unix_signal_semantics: bool,
    pub windows_process_semantics: bool,
    pub strong_process_isolation: bool,
}

impl PlatformCapabilities {
    #[must_use]
    pub fn current() -> Self {
        Self {
            os: std::env::consts::OS.to_owned(),
            architecture: std::env::consts::ARCH.to_owned(),
            native_file_locking: cfg!(any(unix, windows)),
            direct_child_termination: true,
            unix_signal_semantics: cfg!(unix),
            windows_process_semantics: cfg!(windows),
            // Step 05 exposes only the explicitly named DirectHostProcess
            // adapter. Strong sandbox backends are future work and must never
            // be inferred from OS primitives alone.
            strong_process_isolation: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::PlatformCapabilities;

    #[test]
    fn capability_report_is_conservative_and_matches_build_target() {
        let capabilities = PlatformCapabilities::current();
        assert_eq!(capabilities.os, std::env::consts::OS);
        assert_eq!(capabilities.architecture, std::env::consts::ARCH);
        assert_eq!(capabilities.unix_signal_semantics, cfg!(unix));
        assert_eq!(capabilities.windows_process_semantics, cfg!(windows));
        assert!(!capabilities.strong_process_isolation);
    }
}
