from pathlib import Path

workspace = Path("crates/aer-workspace/src/parallel.rs")
s = workspace.read_text(encoding="utf-8")

anchor = '''        if changes.head_commit == changes.base_commit {
            return Err(ParallelWorkspaceError::EmptyTaskChange(
                changes.branch_name.clone(),
            ));
        }
        let previous_head = self.current_head()?;
'''
replacement = '''        if changes.head_commit == changes.base_commit {
            return Err(ParallelWorkspaceError::EmptyTaskChange(
                changes.branch_name.clone(),
            ));
        }

        let branch_head = rev_parse_ref(
            &self.owned.source_repo_root,
            &format!("refs/heads/{}", changes.branch_name),
        )?;
        if branch_head != changes.head_commit {
            return Err(ParallelWorkspaceError::TaskBranchHeadChanged {
                branch: changes.branch_name.clone(),
                expected: changes.head_commit.clone(),
                observed: branch_head,
            });
        }
        if !is_ancestor(
            &self.owned.source_repo_root,
            &changes.base_commit,
            &changes.head_commit,
        )? {
            return Err(ParallelWorkspaceError::TaskBranchBaseMismatch {
                branch: changes.branch_name.clone(),
                base: changes.base_commit.clone(),
                head: changes.head_commit.clone(),
            });
        }
        let observed_paths = changed_paths_between(
            &self.owned.source_repo_root,
            &changes.base_commit,
            &changes.head_commit,
        )?;
        let mut expected_paths = changes.changed_paths.clone();
        expected_paths.sort();
        expected_paths.dedup();
        if observed_paths != expected_paths {
            return Err(ParallelWorkspaceError::TaskChangeSetChanged {
                branch: changes.branch_name.clone(),
            });
        }

        let previous_head = self.current_head()?;
'''
if s.count(anchor) != 1:
    raise SystemExit(f"merge evidence anchor count {s.count(anchor)}")
s = s.replace(anchor, replacement, 1)

anchor = '''fn rev_parse_head(path: &Path) -> Result<String, ParallelWorkspaceError> {
    let result = run_git(
        path,
        [OsString::from("rev-parse"), OsString::from("HEAD")],
        SideEffectClass::PureRead,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    let head = String::from_utf8_lossy(&result.stdout.preview)
        .trim()
        .to_owned();
    if head.is_empty() {
        return Err(ParallelWorkspaceError::UnexpectedEmptyGitOutput);
    }
    Ok(head)
}
'''
replacement = '''fn rev_parse_head(path: &Path) -> Result<String, ParallelWorkspaceError> {
    rev_parse_ref(path, "HEAD")
}

fn rev_parse_ref(path: &Path, reference: &str) -> Result<String, ParallelWorkspaceError> {
    let result = run_git(
        path,
        [OsString::from("rev-parse"), OsString::from(reference)],
        SideEffectClass::PureRead,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    let head = String::from_utf8_lossy(&result.stdout.preview)
        .trim()
        .to_owned();
    if head.is_empty() {
        return Err(ParallelWorkspaceError::UnexpectedEmptyGitOutput);
    }
    Ok(head)
}

fn changed_paths_between(
    repo_root: &Path,
    base: &str,
    head: &str,
) -> Result<Vec<PathBuf>, ParallelWorkspaceError> {
    let result = run_git(
        repo_root,
        [
            OsString::from("diff"),
            OsString::from("--name-only"),
            OsString::from("-z"),
            OsString::from(format!("{base}..{head}")),
            OsString::from("--"),
        ],
        SideEffectClass::PureRead,
        None,
        INSPECTION_OUTPUT_LIMIT,
    )?;
    if result.stdout.truncated {
        return Err(ParallelWorkspaceError::ChangedPathInventoryTooLarge(
            result.stdout.total_bytes,
        ));
    }
    let mut paths = parse_nul_paths(&result.stdout.preview)?;
    paths.sort();
    paths.dedup();
    Ok(paths)
}
'''
if s.count(anchor) != 1:
    raise SystemExit(f"rev parse anchor count {s.count(anchor)}")
s = s.replace(anchor, replacement, 1)

anchor = '''    DirtyTaskWorktree(String),
    EmptyTaskChange(String),
    StaleTaskBase {
'''
replacement = '''    DirtyTaskWorktree(String),
    EmptyTaskChange(String),
    TaskBranchHeadChanged {
        branch: String,
        expected: String,
        observed: String,
    },
    TaskBranchBaseMismatch {
        branch: String,
        base: String,
        head: String,
    },
    TaskChangeSetChanged {
        branch: String,
    },
    StaleTaskBase {
'''
if s.count(anchor) != 1:
    raise SystemExit(f"error enum anchor count {s.count(anchor)}")
s = s.replace(anchor, replacement, 1)

anchor = '''            Self::EmptyTaskChange(branch) => {
                write!(formatter, "task branch has no committed change: {branch}")
            }
            Self::StaleTaskBase {
'''
replacement = '''            Self::EmptyTaskChange(branch) => {
                write!(formatter, "task branch has no committed change: {branch}")
            }
            Self::TaskBranchHeadChanged {
                branch,
                expected,
                observed,
            } => write!(
                formatter,
                "task branch changed after evidence was captured: {branch} expected {expected} observed {observed}"
            ),
            Self::TaskBranchBaseMismatch { branch, base, head } => write!(
                formatter,
                "task branch evidence base is not an ancestor of its head: {branch} {base} -> {head}"
            ),
            Self::TaskChangeSetChanged { branch } => write!(
                formatter,
                "task branch changed-path evidence no longer matches the verified commit range: {branch}"
            ),
            Self::StaleTaskBase {
'''
if s.count(anchor) != 1:
    raise SystemExit(f"display anchor count {s.count(anchor)}")
s = s.replace(anchor, replacement, 1)
workspace.write_text(s, encoding="utf-8")

bench = Path("crates/aer-core/tests/resource_bench.rs")
s = bench.read_text(encoding="utf-8")
anchor = '''    let left_changes = left.change_set().expect("left change set");
    let right_changes = right.change_set().expect("right change set");
    let plan = IntegrationPlan::build(
'''
replacement = '''    let left_changes = left.change_set().expect("left change set");
    let right_changes = right.change_set().expect("right change set");
    let plan = IntegrationPlan::build(
'''
# Keep the existing plan construction unchanged; the mutation test is inserted immediately before the right merge.
if s.count(anchor) != 1:
    raise SystemExit(f"change-set anchor count {s.count(anchor)}")

anchor = '''    let left_merge = integration.merge_task(&left_changes).expect("merge left");
    barrier
        .record_merge("left", left_merge.resulting_head)
        .expect("record left");
    let right_merge = integration.merge_task(&right_changes).expect("merge right");
'''
replacement = '''    let left_merge = integration.merge_task(&left_changes).expect("merge left");
    barrier
        .record_merge("left", left_merge.resulting_head)
        .expect("record left");

    fs::write(right.path().join("src/right.txt"), "right-after-verification\\n")
        .expect("post-verification branch mutation");
    git(right.path(), &["add", "src/right.txt"]);
    git(right.path(), &["commit", "-m", "unverified mutation"]);
    assert!(
        integration.merge_task(&right_changes).is_err(),
        "integration must reject a branch whose HEAD moved after local evidence was captured"
    );
    git(right.path(), &["reset", "--hard", &right_changes.head_commit]);

    let right_merge = integration.merge_task(&right_changes).expect("merge right");
'''
if s.count(anchor) != 1:
    raise SystemExit(f"right merge anchor count {s.count(anchor)}")
s = s.replace(anchor, replacement, 1)
bench.write_text(s, encoding="utf-8")
