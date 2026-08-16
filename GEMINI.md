# Gemini CLI instructions

The canonical coding-agent personality and engineering operating policy for this repository is [`AGENTS.md`](AGENTS.md).

Read `AGENTS.md` first, then follow its required reading order (`STATUS.md`, `docs/00_READ_ME_FIRST.md`, task-relevant architecture docs, and `DEVELOPMENT_PLAN.md` when applicable). Provider authentication, architecture-context, permission and Tool ABI semantics are defined in `docs/45_PROVIDER_AUTH_CONTEXT_PERMISSION_AND_TOOL_RUNTIME.md`.

Do not duplicate or reinterpret those rules here. If this file and `AGENTS.md` ever appear to conflict, `AGENTS.md` is authoritative for implementation temperament; repository architecture authority remains defined by `docs/00_READ_ME_FIRST.md`.

This file is a provider-native compatibility bootstrap only. The everything runtime compiles a bounded, source-hashed Architecture Context Capsule for model calls; provider-native memory/context files are never a substitute for that control-plane context or for AER permission/tool authority. When an AER-managed agent session exposes tools, the typed AER Tool ABI and its permission controller remain authoritative even if Gemini CLI has broader native capabilities.
