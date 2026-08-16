//! Repository-intelligence application boundary.
//!
//! The repository index is derived/rebuildable state. This service binds it to the exact workspace
//! snapshot and current Engineering IR without letting retrieval mutate authoritative project state.

use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

use aer_repo::{
    ImpactCandidate, IndexBuildReport, IndexPolicy, RepoError, RepositoryIndex, SearchQuery,
    SearchResult, SemanticAnchor, SemanticLink,
};
use aer_workspace::{WorkspaceError, WorkspaceIdentity};

use crate::spec::{SpecError, SpecService, SpecSnapshot};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RepositoryRefreshReport {
    pub index: IndexBuildReport,
    pub semantic_links: usize,
}

#[derive(Clone, Debug, Default)]
pub struct RepositoryService {
    policy: IndexPolicy,
}

impl RepositoryService {
    #[must_use]
    pub const fn new(policy: IndexPolicy) -> Self {
        Self { policy }
    }

    pub fn refresh(
        &self,
        workspace_root: impl AsRef<Path>,
        state_home: impl AsRef<Path>,
    ) -> Result<RepositoryRefreshReport, RepositoryServiceError> {
        let workspace = WorkspaceIdentity::inspect(workspace_root.as_ref())?;
        let mut index = self.open_index(&workspace, state_home.as_ref())?;
        let report = index.refresh(&workspace.repo_root)?;

        let spec = SpecService::inspect(&workspace.repo_root, state_home.as_ref())?;
        let anchors = semantic_anchors(&spec);
        let links = index.replace_semantic_anchors(&report.snapshot.snapshot_id, &anchors)?;

        Ok(RepositoryRefreshReport {
            index: report,
            semantic_links: links.len(),
        })
    }

    pub fn search(
        &self,
        workspace_root: impl AsRef<Path>,
        state_home: impl AsRef<Path>,
        query: &SearchQuery,
    ) -> Result<SearchResult, RepositoryServiceError> {
        let workspace = WorkspaceIdentity::inspect(workspace_root.as_ref())?;
        let index = self.open_index(&workspace, state_home.as_ref())?;
        Ok(index.search_current(&workspace.repo_root, query)?)
    }

    pub fn impact(
        &self,
        workspace_root: impl AsRef<Path>,
        state_home: impl AsRef<Path>,
        path: &str,
    ) -> Result<Vec<ImpactCandidate>, RepositoryServiceError> {
        let workspace = WorkspaceIdentity::inspect(workspace_root.as_ref())?;
        let index = self.open_index(&workspace, state_home.as_ref())?;
        let snapshot_id = index
            .current_snapshot_id(&workspace.repo_id)?
            .ok_or_else(|| RepoError::UnknownSnapshot(workspace.repo_id.clone()))?;
        let current = index.search_current(
            &workspace.repo_root,
            &SearchQuery {
                text: path.to_owned(),
                limit: 1,
                min_score_micros: 0,
            },
        )?;
        if current.snapshot_id != snapshot_id {
            return Err(RepositoryServiceError::Repository(RepoError::Integrity(
                "current repository snapshot changed during impact preflight".to_owned(),
            )));
        }
        Ok(index.impact(&snapshot_id, path)?)
    }

    pub fn semantic_links(
        &self,
        workspace_root: impl AsRef<Path>,
        state_home: impl AsRef<Path>,
        semantic_id: &str,
    ) -> Result<Vec<SemanticLink>, RepositoryServiceError> {
        let workspace = WorkspaceIdentity::inspect(workspace_root.as_ref())?;
        let index = self.open_index(&workspace, state_home.as_ref())?;
        let snapshot_id = index
            .current_snapshot_id(&workspace.repo_id)?
            .ok_or_else(|| RepoError::UnknownSnapshot(workspace.repo_id.clone()))?;
        index.search_current(
            &workspace.repo_root,
            &SearchQuery {
                text: semantic_id.to_owned(),
                limit: 1,
                min_score_micros: 0,
            },
        )?;
        Ok(index.semantic_links(&snapshot_id, semantic_id)?)
    }

    fn open_index(
        &self,
        workspace: &WorkspaceIdentity,
        state_home: &Path,
    ) -> Result<RepositoryIndex, RepositoryServiceError> {
        RepositoryIndex::open(
            repository_index_path(state_home, &workspace.repo_id),
            self.policy.clone(),
        )
        .map_err(RepositoryServiceError::Repository)
    }
}

#[must_use]
pub fn repository_index_path(state_home: &Path, repo_id: &str) -> PathBuf {
    super::project_runtime_root(state_home, repo_id)
        .join("repository")
        .join("index.sqlite")
}

fn semantic_anchors(spec: &SpecSnapshot) -> Vec<SemanticAnchor> {
    let Some(ir) = spec.ir.as_ref() else {
        return Vec::new();
    };
    let mut anchors = Vec::new();
    anchors.extend(ir.goals.iter().map(|item| SemanticAnchor {
        kind: "goal".to_owned(),
        id: item.id.clone(),
        text: item.statement.clone(),
    }));
    anchors.extend(
        ir.functional_requirements
            .iter()
            .map(|requirement| SemanticAnchor {
                kind: "requirement".to_owned(),
                id: requirement.item.id.clone(),
                text: requirement.item.statement.clone(),
            }),
    );
    anchors.extend(ir.constraints.iter().map(|item| SemanticAnchor {
        kind: "constraint".to_owned(),
        id: item.id.clone(),
        text: item.statement.clone(),
    }));
    anchors.extend(
        ir.acceptance_criteria
            .iter()
            .map(|criterion| SemanticAnchor {
                kind: "acceptance_criterion".to_owned(),
                id: criterion.item.id.clone(),
                text: criterion.item.statement.clone(),
            }),
    );
    anchors.extend(ir.decisions.iter().map(|decision| SemanticAnchor {
        kind: "decision".to_owned(),
        id: decision.id.clone(),
        text: decision.choice.clone(),
    }));
    anchors.sort_by(|left, right| {
        left.kind
            .cmp(&right.kind)
            .then_with(|| left.id.cmp(&right.id))
    });
    anchors
}

#[derive(Debug)]
pub enum RepositoryServiceError {
    Workspace(WorkspaceError),
    Repository(RepoError),
    Spec(SpecError),
}

impl fmt::Display for RepositoryServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Workspace(error) => write!(f, "repository workspace preflight failed: {error}"),
            Self::Repository(error) => write!(f, "repository intelligence failed: {error}"),
            Self::Spec(error) => write!(f, "repository semantic-anchor sync failed: {error}"),
        }
    }
}

impl Error for RepositoryServiceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Workspace(error) => Some(error),
            Self::Repository(error) => Some(error),
            Self::Spec(error) => Some(error),
        }
    }
}

impl From<WorkspaceError> for RepositoryServiceError {
    fn from(value: WorkspaceError) -> Self {
        Self::Workspace(value)
    }
}

impl From<RepoError> for RepositoryServiceError {
    fn from(value: RepoError) -> Self {
        Self::Repository(value)
    }
}

impl From<SpecError> for RepositoryServiceError {
    fn from(value: SpecError) -> Self {
        Self::Spec(value)
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

    use aer_repo::SearchQuery;

    use super::{RepositoryService, repository_index_path};
    use crate::spec::{SpecService, UserSemanticKind};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        root: PathBuf,
        state: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let nonce = COUNTER.fetch_add(1, Ordering::Relaxed);
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system time")
                .as_nanos();
            let base = std::env::temp_dir().join(format!("aer-core-repo-{now}-{nonce}"));
            let root = base.join("repo");
            let state = base.join("state");
            fs::create_dir_all(root.join("src")).expect("repo directories");
            run(&root, ["init"]);
            run(&root, ["config", "user.email", "aer@example.invalid"]);
            run(&root, ["config", "user.name", "AER Test"]);
            fs::write(
                root.join("src/auth.rs"),
                "pub fn verify_token(token: &str) -> bool { !token.contains(\"expired\") }\n",
            )
            .expect("source");
            run(&root, ["add", "."]);
            run(&root, ["commit", "-m", "initial"]);
            Self { root, state }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(self.root.parent().expect("fixture repository has parent"));
        }
    }

    fn run<const N: usize>(root: &Path, args: [&str; N]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(root)
            .status()
            .expect("git command");
        assert!(status.success());
    }

    #[test]
    fn application_service_keeps_index_outside_workspace_and_links_ir() {
        let fixture = Fixture::new();
        let spec = SpecService::record_semantic(
            &fixture.root,
            &fixture.state,
            UserSemanticKind::Goal,
            "verify expired token authentication",
        )
        .expect("record goal");
        let goal_id = spec.intent.goals[0].id.clone();

        let service = RepositoryService::default();
        let report = service
            .refresh(&fixture.root, &fixture.state)
            .expect("refresh repository intelligence");
        assert!(report.index.text_files >= 1);

        let result = service
            .search(
                &fixture.root,
                &fixture.state,
                &SearchQuery::new("expired token authentication"),
            )
            .expect("search");
        assert!(result.hits.iter().any(|hit| hit.path == "src/auth.rs"));

        let links = service
            .semantic_links(&fixture.root, &fixture.state, &goal_id)
            .expect("semantic links");
        assert!(links.iter().any(|link| link.target_path == "src/auth.rs"));

        let workspace =
            aer_workspace::WorkspaceIdentity::inspect(&fixture.root).expect("workspace identity");
        let index_path = repository_index_path(&fixture.state, &workspace.repo_id);
        assert!(index_path.exists());
        assert!(!index_path.starts_with(&fixture.root));
    }
}
