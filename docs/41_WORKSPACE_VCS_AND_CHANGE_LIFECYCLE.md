# Workspace, VCS, and Change Lifecycle

## 1. Objective

AER must autonomously edit code without taking ownership of the user's working tree.

Git is the preferred change/isolation substrate, but user edits, remotes, branches, hooks, submodules, and dirty state are external authority boundaries.

## 2. Workspace identity

At project attach, record:

```text
repo_id
repo_root
vcs_kind
head_commit
branch/ref
remote identities
dirty_tracked_diff_hash
untracked inventory hash
submodule state
lfs state when relevant
```

This becomes the source for `RepoSnapshot`.

## 3. User working tree rule

AER MUST NOT silently:

- `git reset --hard`,
- discard user edits,
- overwrite untracked files,
- clean ignored files,
- stash and forget state,
- switch the user's active branch,
- rewrite public history.

The user's working tree is not a worker sandbox.

## 4. Dirty tree snapshot

If the user workspace is dirty, AER creates an immutable `WorkspaceSnapshot` describing:

- HEAD/base commit,
- tracked diff artifact,
- selected untracked files where policy allows,
- exclusions/sensitivity labels.

Writable task worktrees are derived from that snapshot without mutating the user's original tree.

If exact reproduction is impossible, implementation pauses or records the limitation instead of pretending a clean commit equals the user's state.

## 5. AER integration branch

AER SHOULD maintain an internal integration ref/branch for accepted project work.

Workers branch from the exact planned snapshot.

Acceptance updates the AER integration state first; applying/pushing to user-owned branches is a separate authority step.

This preserves reversibility.

## 6. Upstream drift

Before integrating a long-running task, compare its base with current integration/upstream state.

Possible outcomes:

```text
clean fast-forward/rebase
text conflict
semantic overlap
dependency invalidation
spec/repository staleness
```

Rebase/merge creates a new repo snapshot and invalidates evidence whose dependency fingerprint changed.

## 7. Conflict resolution

A model receives:

- both change intents,
- exact base/ours/theirs,
- requirement ownership,
- public contract differences,
- relevant evidence.

Conflict resolution MUST be reverified after merge.

Never accept a conflict resolution because the file has no markers.

## 8. Commit discipline

Commits SHOULD be:

- task-linked,
- logically cohesive,
- reversible,
- free of unrelated formatting churn,
- attributable to AER run/task metadata in trailers or structured metadata.

Do not put secrets or raw prompts in commit messages.

## 9. Remote operations

Authority levels are distinct:

```text
local_read
local_worktree_write
local_commit
remote_fetch
remote_branch_push
pull_request_create/update
protected_branch_write
release/tag
```

Local commit permission never implies remote push.

Remote URL changes and credential use require policy checks.

## 10. Pull requests

A PR is an integration surface, not proof.

When AER creates/updates a PR it SHOULD attach concise:

- requirement summary,
- change summary,
- verification/proof link,
- known limitations.

Existing human review/branch protection remains authoritative.

## 11. Submodules, LFS, generated and vendor code

Repository intelligence records special ownership.

AER MUST NOT casually edit:

- generated files when source generator exists,
- vendored dependency trees,
- submodule contents from parent repo,
- LFS pointer/object state,

without task-specific policy.

## 12. Non-git projects

Read-only analysis MAY work without git.

Autonomous writable development requires a reversible snapshot mechanism.

AER MAY offer to initialize git or use an internal snapshot backend, but MUST NOT silently introduce VCS metadata into a user project without policy/user consent.

## 13. Cleanup

Worktree/branch cleanup occurs only after:

- artifacts/evidence preserved,
- accepted changes integrated or explicitly abandoned,
- no active lease references the workspace.

Recovery can identify orphaned worktrees after crashes.

## 14. Tests

Fixtures include:

- dirty tracked/untracked workspace,
- concurrent user edit,
- upstream fast-forward during run,
- rebase conflict,
- semantic conflict without textual conflict,
- submodule/LFS repository,
- symlink/case-sensitivity differences,
- Windows path/locking behavior,
- crash during worktree creation/cleanup.
