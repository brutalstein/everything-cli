use std::{collections::BTreeSet, fmt, str::FromStr};

use aer_exec::SideEffectClass;

/// User-facing prompt policy. This is deliberately separate from the runtime's
/// capability ceiling: changing modes may remove prompts, but it cannot grant a
/// capability that the run/sandbox does not already possess.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionMode {
    Plan,
    Default,
    Auto,
    Full,
}

impl PermissionMode {
    pub const ALL: [Self; 4] = [Self::Plan, Self::Default, Self::Auto, Self::Full];

    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Plan => "plan",
            Self::Default => "default",
            Self::Auto => "auto",
            Self::Full => "full",
        }
    }

    #[must_use]
    pub const fn summary(self) -> &'static str {
        match self {
            Self::Plan => "read-only planning; mutating/process/network actions are denied",
            Self::Default => "reads run automatically; every other eligible action asks",
            Self::Auto => "reads, isolated-worktree edits and local commands run automatically; higher-impact actions ask",
            Self::Full => "all actions already inside the run capability ceiling run automatically",
        }
    }
}

impl fmt::Display for PermissionMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl FromStr for PermissionMode {
    type Err = PermissionError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.trim().to_ascii_lowercase().as_str() {
            "plan" | "read-only" | "readonly" => Ok(Self::Plan),
            "default" | "ask" => Ok(Self::Default),
            "auto" | "workspace" => Ok(Self::Auto),
            "full" | "autonomous" => Ok(Self::Full),
            other => Err(PermissionError::UnknownMode(other.to_owned())),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PermissionDecision {
    Allow,
    Ask,
    Deny,
}

impl PermissionDecision {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Allow => "allow",
            Self::Ask => "ask",
            Self::Deny => "deny",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum GrantScope {
    Once,
    Session,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PermissionRequest {
    pub side_effect: SideEffectClass,
    pub target: String,
    pub reason: String,
    pub reversible: bool,
}

impl PermissionRequest {
    #[must_use]
    pub fn new(
        side_effect: SideEffectClass,
        target: impl Into<String>,
        reason: impl Into<String>,
        reversible: bool,
    ) -> Self {
        Self {
            side_effect,
            target: target.into(),
            reason: reason.into(),
            reversible,
        }
    }
}

/// Session-local permission controller.
///
/// `capability_ceiling` is authority owned by the runtime/sandbox. The model and
/// the permission mode cannot widen it. Explicit session allow/deny choices only
/// affect prompt policy for capabilities already below that ceiling.
#[derive(Clone, Debug)]
pub struct PermissionController {
    mode: PermissionMode,
    capability_ceiling: BTreeSet<SideEffectClass>,
    session_allow: BTreeSet<SideEffectClass>,
    session_deny: BTreeSet<SideEffectClass>,
}

impl PermissionController {
    /// Interactive developer ceiling. Privileged host authority is intentionally
    /// absent and therefore cannot be acquired merely by selecting `full`.
    #[must_use]
    pub fn developer_workspace(mode: PermissionMode) -> Self {
        let capability_ceiling = [
            SideEffectClass::PureRead,
            SideEffectClass::WorkspaceWrite,
            SideEffectClass::ProcessExecution,
            SideEffectClass::NetworkRead,
            SideEffectClass::NetworkWrite,
            SideEffectClass::ExternalMutation,
            SideEffectClass::CredentialUse,
        ]
        .into_iter()
        .collect();
        Self {
            mode,
            capability_ceiling,
            session_allow: BTreeSet::new(),
            session_deny: BTreeSet::new(),
        }
    }

    #[must_use]
    pub const fn mode(&self) -> PermissionMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: PermissionMode) {
        self.mode = mode;
    }

    #[must_use]
    pub fn capability_ceiling(&self) -> &BTreeSet<SideEffectClass> {
        &self.capability_ceiling
    }

    /// Allows callers with higher-level organization/sandbox authority to narrow
    /// the ceiling. This function never widens it.
    pub fn restrict_to<I>(&mut self, allowed: I)
    where
        I: IntoIterator<Item = SideEffectClass>,
    {
        let allowed = allowed.into_iter().collect::<BTreeSet<_>>();
        self.capability_ceiling.retain(|value| allowed.contains(value));
        self.session_allow
            .retain(|value| self.capability_ceiling.contains(value));
    }

    pub fn allow_for_session(&mut self, side_effect: SideEffectClass) -> Result<(), PermissionError> {
        if !self.capability_ceiling.contains(&side_effect) {
            return Err(PermissionError::OutsideCapabilityCeiling(side_effect));
        }
        self.session_deny.remove(&side_effect);
        self.session_allow.insert(side_effect);
        Ok(())
    }

    pub fn deny_for_session(&mut self, side_effect: SideEffectClass) {
        self.session_allow.remove(&side_effect);
        self.session_deny.insert(side_effect);
    }

    pub fn clear_session_override(&mut self, side_effect: SideEffectClass) {
        self.session_allow.remove(&side_effect);
        self.session_deny.remove(&side_effect);
    }

    #[must_use]
    pub fn decide(&self, request: &PermissionRequest) -> PermissionDecision {
        let side_effect = request.side_effect;
        if !self.capability_ceiling.contains(&side_effect)
            || self.session_deny.contains(&side_effect)
        {
            return PermissionDecision::Deny;
        }
        if matches!(self.mode, PermissionMode::Plan) {
            return if side_effect == SideEffectClass::PureRead {
                PermissionDecision::Allow
            } else {
                PermissionDecision::Deny
            };
        }
        if self.session_allow.contains(&side_effect) {
            return PermissionDecision::Allow;
        }
        match self.mode {
            PermissionMode::Plan => unreachable!("plan mode returned above"),
            PermissionMode::Default => {
                if side_effect == SideEffectClass::PureRead {
                    PermissionDecision::Allow
                } else {
                    PermissionDecision::Ask
                }
            }
            PermissionMode::Auto => match side_effect {
                SideEffectClass::PureRead
                | SideEffectClass::WorkspaceWrite
                | SideEffectClass::ProcessExecution => PermissionDecision::Allow,
                SideEffectClass::NetworkRead
                | SideEffectClass::NetworkWrite
                | SideEffectClass::ExternalMutation
                | SideEffectClass::CredentialUse
                | SideEffectClass::Privileged => PermissionDecision::Ask,
            },
            PermissionMode::Full => PermissionDecision::Allow,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PermissionError {
    UnknownMode(String),
    UnknownSideEffect(String),
    OutsideCapabilityCeiling(SideEffectClass),
}

impl fmt::Display for PermissionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownMode(mode) => write!(formatter, "unknown permission mode `{mode}`"),
            Self::UnknownSideEffect(value) => write!(formatter, "unknown side-effect class `{value}`"),
            Self::OutsideCapabilityCeiling(effect) => write!(
                formatter,
                "{effect:?} is outside the current runtime capability ceiling"
            ),
        }
    }
}

impl std::error::Error for PermissionError {}

pub fn parse_side_effect(value: &str) -> Result<SideEffectClass, PermissionError> {
    match value.trim().to_ascii_lowercase().replace('-', "_").as_str() {
        "read" | "pure_read" => Ok(SideEffectClass::PureRead),
        "write" | "workspace_write" => Ok(SideEffectClass::WorkspaceWrite),
        "exec" | "command" | "process_execution" => Ok(SideEffectClass::ProcessExecution),
        "network" | "network_read" => Ok(SideEffectClass::NetworkRead),
        "network_write" => Ok(SideEffectClass::NetworkWrite),
        "external" | "external_mutation" => Ok(SideEffectClass::ExternalMutation),
        "credential" | "credential_use" => Ok(SideEffectClass::CredentialUse),
        "privileged" => Ok(SideEffectClass::Privileged),
        other => Err(PermissionError::UnknownSideEffect(other.to_owned())),
    }
}

#[cfg(test)]
mod tests {
    use aer_exec::SideEffectClass;

    use super::{PermissionController, PermissionDecision, PermissionMode, PermissionRequest};

    fn request(side_effect: SideEffectClass) -> PermissionRequest {
        PermissionRequest::new(side_effect, "fixture", "test", true)
    }

    #[test]
    fn default_allows_reads_and_asks_for_every_other_eligible_effect() {
        let policy = PermissionController::developer_workspace(PermissionMode::Default);
        assert_eq!(
            policy.decide(&request(SideEffectClass::PureRead)),
            PermissionDecision::Allow
        );
        for effect in [
            SideEffectClass::WorkspaceWrite,
            SideEffectClass::ProcessExecution,
            SideEffectClass::NetworkRead,
            SideEffectClass::NetworkWrite,
            SideEffectClass::ExternalMutation,
            SideEffectClass::CredentialUse,
        ] {
            assert_eq!(policy.decide(&request(effect)), PermissionDecision::Ask);
        }
    }

    #[test]
    fn full_removes_prompts_but_cannot_create_privileged_authority() {
        let policy = PermissionController::developer_workspace(PermissionMode::Full);
        assert_eq!(
            policy.decide(&request(SideEffectClass::ExternalMutation)),
            PermissionDecision::Allow
        );
        assert_eq!(
            policy.decide(&request(SideEffectClass::Privileged)),
            PermissionDecision::Deny
        );
    }

    #[test]
    fn explicit_session_deny_wins_even_in_full_mode() {
        let mut policy = PermissionController::developer_workspace(PermissionMode::Full);
        policy.deny_for_session(SideEffectClass::NetworkWrite);
        assert_eq!(
            policy.decide(&request(SideEffectClass::NetworkWrite)),
            PermissionDecision::Deny
        );
    }

    #[test]
    fn plan_is_fail_closed_for_non_read_actions() {
        let policy = PermissionController::developer_workspace(PermissionMode::Plan);
        assert_eq!(
            policy.decide(&request(SideEffectClass::PureRead)),
            PermissionDecision::Allow
        );
        assert_eq!(
            policy.decide(&request(SideEffectClass::ProcessExecution)),
            PermissionDecision::Deny
        );
    }
}
