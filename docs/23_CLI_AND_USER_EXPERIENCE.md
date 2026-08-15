# CLI and User Experience

**Status:** Normative product UX specification  
**Scope:** Interactive CLI/TUI, non-interactive CLI, terminal rendering, user input, permissions, progress, failure, completion, accessibility, and UX testing.  
**Working binary name:** `aer` remains a placeholder until product naming is finalized.

## 1. Product principle

Internal sophistication MUST NOT become user-facing orchestration burden.

AER may coordinate multiple models, tools, worktrees, verifiers, policies, and recovery loops internally. The normal user SHOULD experience one coherent engineering system with one conversational surface, predictable controls, clear progress, and evidence-backed outcomes.

The CLI is the primary product surface, not a debug console around the runtime.

AER MUST optimize the terminal experience for:

1. **clarity before density,**
2. **fast perceived response,**
3. **progressive disclosure,**
4. **calm confidence rather than visual noise,**
5. **reversible user actions,**
6. **explicit trust boundaries,**
7. **automation compatibility,**
8. **terminal portability and graceful degradation.**

“Premium” MUST NOT mean excessive animation, decorative Unicode, constantly moving spinners, or a dashboard that exposes internal machinery. It means a surface that feels deliberate, fast, stable, legible, and trustworthy.

---

## 2. Interaction model

AER has two first-class surfaces:

### 2.1 Interactive workspace

Running:

```text
aer
```

enters the conversational project workspace when stdout/stdin are attached to a capable TTY.

This surface is optimized for:

- describing goals in natural language,
- resolving only high-impact ambiguity,
- reviewing the compiled project contract,
- starting and supervising execution,
- answering permission or product questions,
- inspecting progress,
- reviewing evidence and completion state,
- resuming long-running work.

### 2.2 Command surface

Commands remain available for explicit control, scripting, debugging, and automation:

```text
aer init
aer build "<goal>"
aer resume
aer status
aer stop
aer doctor
```

Inspection:

```text
aer inspect project
aer inspect task <id>
aer inspect context <id>
aer inspect route <id>
aer inspect evidence <id>
aer inspect proof <id>
aer inspect cost [run]
aer inspect health
aer inspect events [run]
```

Model/policy:

```text
aer models
aer models benchmark
aer policy show
aer config get
aer config set
aer eval run <suite>
```

The command surface MUST remain stable even if the interactive UI changes.

---

## 3. UX architecture

The UI MUST be a projection of runtime state, never the owner of runtime state.

Use an explicit boundary:

```text
Runtime domain events
        │
        ▼
UI projection / view model
        │
        ├── interactive TUI renderer
        ├── line-mode renderer
        └── JSON / machine renderer
```

The renderer MUST NOT directly mutate task, routing, evidence, or policy state.

User actions MUST be converted into typed commands and submitted to the runtime through the same application boundary used by non-interactive clients.

This separation enables:

- deterministic replay,
- snapshot testing,
- headless testing,
- multiple frontends,
- reliable recovery after terminal crashes,
- consistent semantics between TUI and `--json`.

The interactive client MAY disappear without terminating the underlying durable run unless the user explicitly requests stop/cancel.

---

## 4. Recommended Rust implementation baseline

The architecture MUST remain library-replaceable, but the initial Rust implementation SHOULD use:

- `clap` for stable command parsing and help generation;
- `ratatui` for full-screen interactive terminal composition;
- `crossterm` as the default cross-platform terminal backend.

Versions MUST be controlled by the repository lockfile and dependency policy rather than copied from this document.

The full-screen TUI MUST only activate after terminal capability detection. Unsupported terminals, redirected streams, dumb terminals, CI, or accessibility modes MUST receive a clean line-oriented interface instead.

The implementation MUST have an abstraction over terminal capabilities so a specific TUI library is not part of the domain architecture.

---

## 5. Terminal capability negotiation

At startup, detect capabilities before rendering advanced output.

At minimum account for:

- interactive TTY vs redirected stdin/stdout/stderr,
- terminal dimensions,
- ANSI/color support,
- Unicode capability,
- alternate-screen support,
- mouse support if ever enabled,
- OSC 8 hyperlink capability where safely detectable,
- reduced-color environments,
- `NO_COLOR`,
- CI/non-interactive execution,
- Windows Terminal / PowerShell environments,
- SSH and multiplexers,
- narrow or very small terminals.

Never assume truecolor.

The UI MUST degrade in layers:

```text
full TUI
  ↓
compact interactive line mode
  ↓
plain text
  ↓
structured JSON
```

All four modes MUST preserve the same semantic state and user decisions.

Color and symbols MUST be supplementary, never the only carrier of meaning.

---

## 6. Visual language

AER should look like a professional engineering instrument, not a game HUD.

### 6.1 Palette

Use a restrained semantic palette.

Conceptual roles:

- neutral text,
- muted secondary text,
- accent / active focus,
- success,
- warning,
- failure,
- blocked / policy state.

Do not encode model/provider identity primarily by color.

Avoid rainbow output and multi-color log spam.

Color MUST respect terminal defaults where practical and MUST remain readable on both dark and light terminal themes.

### 6.2 Typography and hierarchy

Terminal typography is controlled by the user, so hierarchy comes from:

- whitespace,
- alignment,
- concise labels,
- restrained weight/intensity,
- borders only when they clarify grouping,
- short semantic status tokens.

Avoid giant ASCII logos after first-run onboarding.

Branding SHOULD be compact enough that it does not push useful information below the fold.

### 6.3 Symbols

Unicode symbols MAY improve scanning but MUST have ASCII fallbacks.

Example semantic vocabulary:

```text
✓ accepted
● running
○ ready
! needs attention
× failed
↻ retrying
… waiting
```

The actual glyph set SHOULD be centralized in the UI theme layer rather than scattered through application code.

No state may rely on glyph shape alone.

---

## 7. Perceived performance budget

Premium terminal UX is primarily latency discipline.

Excluding network/model response time, target:

| Interaction | Target |
|---|---:|
| command parse + help/error response | p95 <= 100 ms |
| interactive first paint, warm runtime | p95 <= 150 ms |
| interactive first paint, cold local runtime | p95 <= 350 ms |
| keypress-to-visible-update | p95 <= 50 ms |
| resize-to-stable-layout | p95 <= 100 ms |
| local navigation / panel change | p95 <= 50 ms |
| cancel acknowledgement | <= 100 ms |

These are engineering targets, not reasons to fake progress.

Rendering SHOULD be event-driven. Do not redraw at high frame rates when nothing changes.

Animated indicators SHOULD generally update around 8–12 Hz or lower. Idle screens SHOULD consume negligible CPU.

The UI MUST immediately acknowledge long operations with a semantic state transition before model/network work completes.

Bad:

```text
<blank screen for six seconds>
```

Good:

```text
Understanding project…
```

then transition to real milestones as evidence arrives.

Never fabricate percent-complete values when the runtime cannot estimate completion honestly.

---

## 8. Main interactive layout

The default interactive experience SHOULD fit comfortably in a normal developer terminal without requiring maximization.

Conceptual layout:

```text
┌──────────────────────────────────────────────────────────────┐
│ AER  /workspace/project                         run 01J…     │
│ Building realtime meeting assistant                         │
├──────────────────────────────────────────────────────────────┤
│                                                              │
│ AER                                                          │
│ I need one product decision before I freeze the contract.    │
│ Should meeting analysis happen live, after upload, or both?  │
│                                                              │
│ You                                                          │
│ both                                                         │
│                                                              │
│ ✓ Product behavior resolved                                  │
│ ● Compiling engineering contract…                            │
│                                                              │
├──────────────────────────────────────────────────────────────┤
│ > Type a message…                               ctrl+k actions│
└──────────────────────────────────────────────────────────────┘
```

The product MUST not permanently reserve large panes for:

- model names,
- token counters,
- raw logs,
- agent lists,
- shell output,
- context chunks,
- debug traces.

Those belong behind inspection views.

The normal view is **conversation + semantic project progress**.

---

## 9. Conversational onboarding

The first-use flow MUST not ask users to configure agent architecture.

Expected:

```text
$ aer

What do you want to build?

> A realtime meeting assistant that extracts decisions and tasks.

AER
One product behavior materially changes the architecture:
should analysis happen live during the meeting, after upload, or both?

> both

AER
Understood. I can make the remaining infrastructure choices from
engineering defaults.

✓ 12 required behaviors
✓ 4 quality constraints
✓ 2 explicit non-goals
✓ 1 assumption recorded

Review contract  Enter
Edit            E
Start           S
```

Only questions with meaningful expected information gain SHOULD interrupt the user.

Do not ask questions such as framework/database/provider preferences unless:

- the user expressed a preference,
- an existing repository constrains the choice,
- the decision has material product consequences,
- or policy requires explicit consent.

When a safe, conventional engineering default exists, AER SHOULD choose it and record the rationale.

---

## 10. Input composer

The input surface is central to perceived quality.

It SHOULD support:

- multiline input,
- bracketed paste,
- large paste detection,
- editable history,
- shell-safe text handling,
- UTF-8,
- keyboard-only operation,
- command completion for explicit commands,
- file/path mention completion where useful,
- clear handling of pasted stack traces or code,
- draft preservation across temporary view changes.

Suggested behavior:

- `Enter`: submit when input is a normal single-line message;
- `Shift+Enter` or configured equivalent: newline where terminal support permits;
- explicit multiline mode MUST exist for terminals that cannot distinguish modified Enter;
- `Esc`: close transient overlay / return focus, never silently cancel the run;
- `Ctrl+C`: first interrupts the current foreground interaction; repeated use MAY request run cancellation with confirmation according to state;
- `Ctrl+D`: exit the interactive client when the composer is empty; MUST NOT silently destroy durable work;
- `Ctrl+K`: open an action palette in capable TTY mode.

Keybindings MUST be discoverable and configurable. Essential actions MUST also be reachable through textual commands.

---

## 11. Action palette and progressive disclosure

Advanced capabilities SHOULD be discoverable without becoming persistent chrome.

A compact action palette MAY expose:

```text
Resume run
Pause / stop run
Review project contract
Open task graph
Inspect evidence
Inspect cost
Inspect architecture health
Change autonomy policy
Open logs
Copy run ID
Exit client
```

The palette MUST issue typed runtime commands; it is not a shortcut around policy or permission checks.

Slash commands MAY exist for expert users, but natural language remains the primary interaction.

---

## 12. Semantic progress, not activity theater

Normal progress output MUST represent engineering meaning.

Example:

```text
meeting-assistant                                              01J…
──────────────────────────────────────────────────────────────────
✓ Specification contract                         accepted
✓ Repository bootstrap                           verified
● Realtime ingestion                             implementing
○ Persistence layer                              ready
! UI integration                                 waiting on API contract

Verification   31 / 34 required checks
Architecture   healthy
Budget         $4.82 used · $12.00 policy limit
```

Do not print:

- every model turn,
- every internal agent message,
- hidden reasoning,
- every shell command,
- raw chain-of-thought,
- every context retrieval,
- token accounting after every request.

AER SHOULD coalesce high-frequency runtime events into stable semantic milestones.

Detailed execution remains inspectable.

---

## 13. Truthful waiting states

Every wait state MUST answer, when possible:

1. **what is happening,**
2. **what the system is waiting for,**
3. **whether the user can act,**
4. **whether work is still progressing.**

Examples:

```text
● Verifying authentication changes
  184 tests running
```

```text
… Waiting for API rate limit
  retry eligible in 22s · no user action needed
```

```text
! Decision required
  Production data migration is destructive.
```

Avoid generic indefinite spinners such as “Thinking…” for long periods when more precise state exists.

Never expose fabricated reasoning narration merely to make the UI look busy.

---

## 14. Permission and trust UX

Permission prompts are security boundaries and MUST be visually distinct from ordinary conversation.

A permission request MUST include:

- requested action,
- target/scope,
- concrete reason,
- risk class,
- whether the action is reversible,
- least-privilege alternatives when available,
- approval duration/scope.

Example:

```text
┌ Permission required ─────────────────────────────────────────┐
│ Run: npm publish                                            │
│ Target: package @example/sdk                                │
│ Reason: release task REL-12                                 │
│ Risk: external write · publicly visible                    │
│                                                             │
│ [1] Allow once   [2] Deny   [3] Inspect command            │
└─────────────────────────────────────────────────────────────┘
```

“Always allow” MUST NOT be casually offered for high-impact capabilities.

Secrets MUST never be echoed in permission UI.

A denied permission is a normal runtime state, not an exceptional crash.

---

## 15. Failure UX

Errors MUST be actionable and layered.

The first surface SHOULD show:

```text
× Build verification failed

2 required checks did not pass.
AER has preserved the worktree and evidence.

Enter  View failures
R      Attempt recovery
D      Open detailed diagnostics
```

Detailed view MAY include:

- failed checks,
- affected requirement IDs,
- relevant command output,
- file references,
- recovery attempts,
- evidence IDs,
- policy decision.

Avoid dumping a multi-thousand-line stack trace by default.

Unexpected internal crashes MUST restore terminal state before printing diagnostics.

When alternate-screen mode is used, terminal restoration MUST be guarded so panic/error paths do not leave the user's shell corrupted.

---

## 16. Completion UX

“Done” is not an animation. It is an evidence state.

Completion SHOULD summarize:

```text
✓ Project change accepted

Requirements      17 / 17 satisfied
Verification      284 / 284 required checks passed
Architecture      no blocking regression
Security          required gates passed
Cost              $6.31
Duration           18m 42s

Changed            14 files
Proof manifest     proof://01J…
Run                 01J…

Enter  Review changes
P      Open proof
D      View diff
```

Do not celebrate work that has not passed its required verification policy.

Do not use confetti-like terminal effects.

---

## 17. Inspection views

Inspection is an expert layer.

### `aer inspect route TASK-42`

Show:

- eligible models,
- selected model,
- model capability evidence,
- expected cost/quality tradeoff,
- escalation history,
- policy version.

Do not show private chain-of-thought.

### `aer inspect context TASK-42`

Show:

- source references,
- selected context items,
- token cost,
- ranking features/reasons,
- provenance,
- exclusions when diagnostically useful.

### `aer inspect evidence TASK-42`

Show immutable evidence identifiers and their relationship to requirements and changes.

### `aer inspect cost`

Show meaningful aggregation first, with drilldown by model/task/tool only when requested.

---

## 18. Responsive terminal layout

The interface MUST react correctly to terminal width and height.

Suggested breakpoints are behavioral rather than fixed design constants:

### Wide

May show a secondary compact status region.

### Standard

Conversation + status footer; details remain overlays.

### Narrow

Collapse borders and secondary labels. Prefer one-column content.

### Very small

Exit full-screen layout and present a clear compact mode rather than rendering broken panels.

Text MUST wrap predictably.

Tables MUST degrade to key/value lists before columns become unreadable.

No essential action may require horizontal scrolling.

---

## 19. Accessibility

Accessibility is a core requirement.

AER MUST support:

- `NO_COLOR`,
- explicit `--color=auto|always|never`,
- a plain/linear rendering mode,
- text labels in addition to color/symbol state,
- configurable/reduced motion,
- keyboard-only use,
- copyable text output,
- readable focus indication,
- screen-reader-friendly line mode,
- no reliance on rapid blinking.

The UI SHOULD allow:

```text
aer config set ui.motion reduced
aer config set ui.mode line
aer config set ui.unicode false
```

Exact configuration keys may be finalized with the configuration schema.

---

## 20. Logs and scrollback

Full-screen mode SHOULD preserve the user's shell scrollback by using the terminal's alternate screen where supported.

AER MUST still make significant run information durable outside the screen buffer.

The user MUST be able to retrieve prior semantic events with:

```text
aer inspect events <run>
```

Raw runtime logs SHOULD be stored separately from human semantic events.

Exiting the TUI MUST leave a concise shell-safe summary so the user is not left with an empty terminal and no run identifier.

Example:

```text
AER run 01J… continues in background
status: aer status
resume: aer resume 01J…
```

---

## 21. Notifications

Terminal bells and desktop notifications MUST be opt-in or policy-controlled.

Useful notification classes:

- input required,
- permission required,
- run completed,
- run failed,
- budget/security stop.

Do not notify for routine agent/tool transitions.

Notification content MUST not leak secrets.

---

## 22. Headless and automation mode

Interactive polish MUST never compromise composability.

Supported:

```text
aer build --non-interactive --spec project.yaml
aer status --json
aer proof --json
```

Machine output MUST be stable, versioned where necessary, and separated from human diagnostics.

For `--json`:

- stdout = machine payload;
- stderr = diagnostics;
- no ANSI;
- no spinner;
- no decorative text;
- stable exit code.

If non-interactive input contains unresolved high-impact ambiguity, return a structured `needs_input` result unless policy defines a safe default.

---

## 23. Exit codes

Stable exit categories:

```text
0 success / accepted
2 needs user input
3 verification failed
4 policy / security blocked
5 environment / setup failure
6 budget exhausted
7 internal runtime failure
```

Exact numbering may be finalized in an ADR, but once released publicly it becomes compatibility surface.

---

## 24. Help UX

`aer --help` MUST be concise enough to scan.

The first screen SHOULD emphasize high-frequency commands and examples, not every configuration flag.

Use layered help:

```text
aer --help
aer build --help
aer inspect --help
aer help workflows
```

Errors SHOULD suggest the closest valid command or argument when confidence is high.

Bad:

```text
error: unexpected argument
```

Better:

```text
Unknown command `stats`.

Did you mean `status`?
Run `aer --help` for commands.
```

Shell completion SHOULD eventually be provided for major shells on supported platforms.

---

## 25. UX state machine

The client SHOULD model explicit UI states instead of ad-hoc booleans.

Minimum conceptual states:

```text
Boot
WorkspaceReady
Interviewing
ContractReview
Executing
WaitingForUser
WaitingForPermission
Verifying
Recovering
Completed
Failed
Disconnected
```

Transient overlays such as help, action palette, logs, and inspectors are orthogonal view state and MUST NOT mutate the underlying run state.

Every transition SHOULD be driven by typed runtime events or typed user commands.

---

## 26. Reconnection and durability

The interface MUST assume runs can outlive terminals.

On client restart:

```text
$ aer

Found an active run for this workspace.

● realtime ingestion       implementing
✓ persistence layer        verified
! production deploy        permission required

Enter  Resume
N      Start new run
```

The TUI MUST reconstruct its visible state from durable runtime projections, not from an in-memory chat transcript.

Network/provider interruptions SHOULD appear as recoverable engineering state when the runtime can retry safely.

---

## 27. Observability without clutter

AER SHOULD expose a compact optional status line containing only high-value signals:

```text
balanced · $4.82/$12 · 31/34 verified · 2 workers
```

Model/provider names SHOULD not be permanent primary chrome unless the user enabled expert telemetry.

Token counts are diagnostics, not product progress.

If cost cannot be known accurately, say `estimating` or omit it rather than display false precision.

---

## 28. UX testing strategy

The CLI/TUI requires dedicated tests; ordinary backend coverage is insufficient.

Required layers:

### 28.1 Pure projection tests

Given a sequence of domain events, assert the resulting UI view model.

These tests MUST not require a real terminal.

### 28.2 Renderer snapshots

Snapshot representative terminal frames across:

- widths/heights,
- dark/light compatible palettes,
- Unicode and ASCII modes,
- color/no-color,
- all critical states,
- long paths,
- long model/user messages,
- wrapping edge cases.

Snapshots MUST be reviewed semantically; visual churn alone is not success.

### 28.3 PTY interaction tests

Launch the real executable in a pseudo-terminal and verify:

- key handling,
- paste,
- resize,
- Ctrl+C/Ctrl+D behavior,
- alternate-screen restoration,
- permission flows,
- crash recovery,
- resume behavior.

### 28.4 Headless contract tests

Assert exact JSON shape, stderr separation, and exit codes.

### 28.5 Fuzz/property tests

Fuzz:

- resize sequences,
- arbitrary Unicode,
- malformed terminal dimensions,
- high-rate event bursts,
- repeated reconnect/disconnect,
- input editing,
- projection replay.

Property: no event sequence may cause terminal-state corruption or mutate domain state through rendering.

### 28.6 Performance tests

Continuously measure:

- cold/warm first paint,
- key-to-render latency,
- resize latency,
- idle CPU,
- memory growth during long sessions,
- event coalescing under burst load.

Do not approve UI features that violate the latency budget without explicit evidence and rationale.

---

## 29. Anti-patterns

AER MUST NOT ship a CLI that:

- resembles raw CI logs;
- prints hidden reasoning;
- constantly scrolls while nothing meaningful changes;
- opens five permanent panes for internal agents;
- uses fake percent-complete values;
- blocks startup on decorative network work;
- requires a mouse;
- breaks when stdout is piped;
- mixes JSON and human formatting on stdout;
- loses typed user input when switching views;
- makes Ctrl+C unpredictably destroy durable work;
- leaves the terminal in raw/alternate mode after a crash;
- treats color as the only status signal;
- exposes secrets in logs, prompts, or clipboard helpers;
- asks users to choose models/agents for ordinary tasks;
- forces users to understand the internal orchestration graph.

---

## 30. Acceptance criteria for “premium”

The interactive CLI is not considered product-ready until all of the following are true:

1. A new user can start a project from one natural-language goal without learning agent terminology.
2. High-impact ambiguity is surfaced conversationally; low-value configuration questions are not.
3. The interface acknowledges local actions inside the latency budget.
4. Long-running work shows semantic state without log spam or fabricated progress.
5. Every permission request explains action, scope, reason, and risk.
6. TUI state survives resize, disconnect, resume, and ordinary terminal interruption.
7. Full-screen, line, plain, and JSON modes preserve equivalent semantics.
8. Core workflows are usable without color, Unicode, mouse, or animation.
9. Completion is backed by verification evidence and exposes the proof manifest.
10. Raw execution details are available on demand but absent from the default surface.
11. The client can crash/restart without losing the authoritative run.
12. UX behavior has projection, snapshot, PTY, headless, and performance coverage.
13. The terminal is always restored correctly after normal exit, cancellation, panic, and runtime failure.
14. The normal user never has to manually design an agent graph or decide which model should do routine work.
15. The interface remains calm and legible during highly parallel internal execution.

---

## 31. Final product invariant

> The user should feel that one exceptionally capable engineering system is working with them, even when the runtime internally coordinates many models, tools, verifiers, and workers.

The runtime may be complex. The surface MUST remain simple.

The UI should expose **intent, decisions, progress, risk, evidence, and control**—not orchestration machinery.
