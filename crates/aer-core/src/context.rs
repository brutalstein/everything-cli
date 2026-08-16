//! Context Economy Engine application boundary.
//!
//! This service binds a Context Pack to the current authoritative Engineering IR and exact
//! repository snapshot. It owns no presentation behavior and does not mutate user semantics.

use std::{error::Error, fmt, path::Path};

use aer_context::{ContextEngine, ContextError, ContextPack, ContextPolicy, ContextRequest};
use aer_repo::{IndexPolicy, RepoError, RepositoryIndex};
use aer_workspace::{WorkspaceError, WorkspaceIdentity};

use crate::{
    repository::{RepositoryService, RepositoryServiceError, repository_index_path},
    spec::{SpecError, SpecService},
};

pub struct ContextService {
    repository_policy: IndexPolicy,
    engine: ContextEngine,
}

impl ContextService {
    pub fn new(
        repository_policy: IndexPolicy,
        context_policy: ContextPolicy,
    ) -> Result<Self, ContextServiceError> {
        Ok(Self {
            repository_policy,
            engine: ContextEngine::new(context_policy)?,
        })
    }

    pub fn standard() -> Result<Self, ContextServiceError> {
        Self::new(IndexPolicy::default(), ContextPolicy::default())
    }

    pub fn compile(
        &self,
        workspace_root: impl AsRef<Path>,
        state_home: impl AsRef<Path>,
        request: &ContextRequest,
    ) -> Result<ContextPack, ContextServiceError> {
        let workspace_root = workspace_root.as_ref();
        let state_home = state_home.as_ref();
        let spec = SpecService::inspect(workspace_root, state_home)?;
        if spec.ir.is_none() || spec.revision == 0 {
            return Err(ContextServiceError::MissingEngineeringIr);
        }
        if request.engineering_ir_version != spec.revision {
            return Err(ContextServiceError::IrVersionMismatch {
                requested: request.engineering_ir_version,
                current: spec.revision,
            });
        }

        RepositoryService::new(self.repository_policy.clone()).refresh(workspace_root, state_home)?;
        let workspace = WorkspaceIdentity::inspect(workspace_root)?;
        let index = RepositoryIndex::open(
            repository_index_path(state_home, &workspace.repo_id),
            self.repository_policy.clone(),
        )?;
        Ok(self.engine.compile(&workspace.repo_root, &index, request)?)
    }

    pub fn verify_fidelity(
        &self,
        workspace_root: impl AsRef<Path>,
        state_home: impl AsRef<Path>,
        pack: &ContextPack,
    ) -> Result<(), ContextServiceError> {
        let workspace_root = workspace_root.as_ref();
        let state_home = state_home.as_ref();
        let spec = SpecService::inspect(workspace_root, state_home)?;
        if spec.revision != pack.engineering_ir_version {
            return Err(ContextServiceError::IrVersionMismatch {
                requested: pack.engineering_ir_version,
                current: spec.revision,
            });
        }
        let workspace = WorkspaceIdentity::inspect(workspace_root)?;
        let index = RepositoryIndex::open(
            repository_index_path(state_home, &workspace.repo_id),
            self.repository_policy.clone(),
        )?;
        self.engine.verify_fidelity(&workspace.repo_root, &index, pack)?;
        Ok(())
    }
}

#[derive(Debug)]
pub enum ContextServiceError {
    MissingEngineeringIr,
    IrVersionMismatch { requested: u64, current: u64 },
    Workspace(WorkspaceError),
    RepositoryService(RepositoryServiceError),
    Repository(RepoError),
    Spec(SpecError),
    Context(ContextError),
}

impl fmt::Display for ContextServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingEngineeringIr => {
                write!(f, "Context Pack compilation requires a current Engineering IR")
            }
            Self::IrVersionMismatch { requested, current } => write!(
                f,
                "Context Pack Engineering IR version is stale: requested {requested}, current {current}"
            ),
            Self::Workspace(error) => write!(f, "context workspace preflight failed: {error}"),
            Self::RepositoryService(error) => {
                write!(f, "context repository refresh failed: {error}")
            }
            Self::Repository(error) => write!(f, "context repository access failed: {error}"),
            Self::Spec(error) => write!(f, "context specification access failed: {error}"),
            Self::Context(error) => write!(f, "context compilation failed: {error}"),
        }
    }
}

impl Error for ContextServiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Workspace(error) => Some(error),
            Self::RepositoryService(error) => Some(error),
            Self::Repository(error) => Some(error),
            Self::Spec(error) => Some(error),
            Self::Context(error) => Some(error),
            Self::MissingEngineeringIr | Self::IrVersionMismatch { .. } => None,
        }
    }
}

impl From<WorkspaceError> for ContextServiceError {
    fn from(value: WorkspaceError) -> Self {
        Self::Workspace(value)
    }
}

impl From<RepositoryServiceError> for ContextServiceError {
    fn from(value: RepositoryServiceError) -> Self {
        Self::RepositoryService(value)
    }
}

impl From<RepoError> for ContextServiceError {
    fn from(value: RepoError) -> Self {
        Self::Repository(value)
    }
}

impl From<SpecError> for ContextServiceError {
    fn from(value: SpecError) -> Self {
        Self::Spec(value)
    }
}

impl From<ContextError> for ContextServiceError {
    fn from(value: ContextError) -> Self {
        Self::Context(value)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    use aer_context::ContextRequest;

    use super::{ContextService, ContextServiceError};
    use crate::spec::{SpecService, UserSemanticKind};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        base: PathBuf,
        repo: PathBuf,
        state: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let base = std::env::temp_dir().join(format!("aer-core-context-{now}-{nonce}"));
            let repo = base.join("repo");
            let state = base.join("state");
            fs::create_dir_all(repo.join("src")).expect("src");
            fs::create_dir_all(repo.join("tests")).expect("tests");
            git(&repo, ["init"]);
            git(&repo, ["config", "user.email", "aer@example.invalid"]);
            git(&repo, ["config", "user.name", "AER Test"]);
            fs::write(
                repo.join("src/auth.rs"),
                "pub fn verify_token(token: &str) -> bool { !token.contains(\"expired\") }\n",
            )
            .expect("source");
            fs::write(
                repo.join("tests/auth_test.rs"),
                "#[test]\nfn expired_token_is_rejected(){ assert!(!verify_token(\"expired\")); }\n",
            )
            .expect("test");
            git(&repo, ["add", "."]);
            git(&repo, ["commit", "-m", "initial"]);
            Self { base, repo, state }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    fn git<const N: usize>(repo: &Path, args: [&str; N]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo)
            .status()
            .expect("git command");
        assert!(status.success());
    }

    #[test]
    fn application_context_pack_is_bound_to_current_ir_and_repository() {
        let fixture = Fixture::new();
        let spec = SpecService::record_semantic(
            &fixture.repo,
            &fixture.state,
            UserSemanticKind::Goal,
            "expired authentication tokens must be rejected and verified by tests",
        )
        .expect("goal");
        let goal_id = spec.intent.goals[0].id.clone();
        let mut request = ContextRequest::new(
            "task-auth",
            "fix expired authentication token verification and its tests",
            spec.revision,
            1000,
        );
        request.required_semantic_ids.push(goal_id);

        let service = ContextService::standard().expect("context service");
        let pack = service
            .compile(&fixture.repo, &fixture.state, &request)
            .expect("compile pack");
        assert_eq!(pack.engineering_ir_version, spec.revision);
        assert!(pack.items.iter().any(|item| item.path == "src/auth.rs"));
        assert!(pack.total_token_cost() <= request.input_token_budget);
        service
            .verify_fidelity(&fixture.repo, &fixture.state, &pack)
            .expect("verify pack");
    }

    #[test]
    fn stale_ir_version_is_rejected_before_context_compilation() {
        let fixture = Fixture::new();
        let spec = SpecService::record_semantic(
            &fixture.repo,
            &fixture.state,
            UserSemanticKind::Goal,
            "verify authentication tokens",
        )
        .expect("goal");
        let request = ContextRequest::new(
            "task-auth",
            "verify authentication tokens",
            spec.revision.saturating_add(1),
            800,
        );
        let service = ContextService::standard().expect("context service");
        let error = service
            .compile(&fixture.repo, &fixture.state, &request)
            .expect_err("stale IR version must fail");
        assert!(matches!(
            error,
            ContextServiceError::IrVersionMismatch { .. }
        ));
    }
}
