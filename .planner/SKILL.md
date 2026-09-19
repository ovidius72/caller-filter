<!-- agent-plan-managed-skill sha256:2c9cfd58ad714a2b9640f3b7fb21183f0394532547d940aae383a3514fb621d5 -->
---
name: agent-plan
summary: Cross-harness operating guide for Agent Plan projects.
---

# Agent Plan operating guide

This is the canonical, project-local operating guide for Agent Plan. A managed copy lives at `.planner/SKILL.md`. Treat it as agent-only operational context: read it when the planner is explicitly loaded, follow it while working, and do not quote it in the human-facing load recap.

## Activation and ownership

- The planner and dashboard are disabled by default. Load them only after an explicit user request: Pi `/planner load` or `planner-load`; MCP `planner-load`.
- Starting only the dashboard does not enable planner context. Pi uses `planner-web`; MCP uses `planner-web`.
- Stop Pi planner context and its dashboard with `/planner stop`, `planner-stop`, or `/planner disable`. For MCP, `planner-disable` explains how to disable the server.
- `.planner/` is the operational source of truth. Keep feature, phase, task, requirement, decision, checklist, handoff, and Project Guidelines state synchronized with completed work.
- Planner metadata operations are not code edits. Code, configuration, repository, dependency, or environment changes still require the project’s own approval rules.
- The Web UI is a human-supervisor surface. It may bypass agent-only governance or motivation gates. Agents must not imitate its source header or treat its exemptions as an agent bypass.

## References, discovery, and priority

Use human references instead of raw UUIDs:

- Feature: `F001`
- Feature phase: `P001(F001)`
- Task: `P001(F001)/T001`
- Global phase/task: `P001` and `P001/T001`
- Short IDs and exact titles may be accepted when unambiguous, but composite references are safest across projects and sessions.

Discover before mutating:

1. List features, phases, or tasks using compact list tools.
2. Follow the lowest visible ready priority unless the automatic recommendation or an approved deviation says otherwise.
3. Use `task_recommend` / `planner-task-recommend` when choosing the next task. Treat its `claims` as bounded evidence: priority is a policy signal, a handoff is actionable only when persisted `resumeReady=true`, and Markdown prose or terminal archives never create an action claim.
4. Read the exact entity with its full-detail show/get surface when full context is needed.
5. Never infer an ambiguous bare reference. Ask for the exact composite reference.
6. Never claim that no task is active from counts, feature summaries, or omitted task detail. Use an explicit `activeTaskState`/`activeTasks` result from `plan_get`, `planner-show`, or the lifecycle recommendation. Only `activeTaskState: none` (verified from all persisted task statuses) proves absence; `conflict` means multiple active tasks must be reconciled.

Feature and phase statuses are derived from their children. Do not write their status directly; update the relevant child tasks. A `DERIVED_STATUS_READ_ONLY` result is a non-success result.

## Lifecycle-first context protocol

Before touching code, call `task_start` / `planner-task-start`, or use `task_switch` / `planner-task-switch` when another task is already active. The lifecycle tool may deny the transition and return typed diagnostics.

When denied:

1. Confirm `started` is `false` and read `errorCode` plus `nextActions`.
2. Perform only the missing or stale reads listed in `nextActions`. Reads may be completed in any order within the current session.
3. If Project Guidelines are listed, call `project_guidelines_show` or `planner-project-guidelines-show` and retain the content while working.
4. Read each task on every start or resume. Fresh unchanged feature, phase, and linked-requirement reads may be reused across sibling tasks in the same session. When linked requirements are requested, call `requirement_list` or `planner-requirement-list` with the exact `phaseRef` from `nextActions`; a broad unscoped inventory does not attest that every requirement was read. A full feature/phase/task read is complete only when it delivers all canonical Accepted Decision fields (`id`, `title`, `decision`, `rationale`, `implementationNotes`, `acceptedAt`); title-only summaries never satisfy the read gate.
5. Use the returned priority-ordered phase work map to review sibling task refs, goals, dependencies, statuses, and remaining capability ownership. Before proposing or creating work, reread the canonical phase and the relevant sibling task full view; never duplicate a capability already owned by another task.
6. Retry the lifecycle operation. Only `started: true` proves work is active.

Do not convert a denial into a planner status change merely to bypass the gate. Common typed denials include `PROJECT_GUIDELINES_READ_REQUIRED`, `CONTEXT_READ_REQUIRED`, `REQUIREMENTS_READ_REQUIRED`, `START_NOT_ALLOWED`, `ACTIVE_TASK_CONFLICT`, `TASK_DONE`, and persistence verification failures.

## Project Guidelines

`Project Guidelines` is the canonical project section for coding standards, formatting, styling, verification conventions, and other implementation rules.

`Requirements` are separate declarative product outcomes: user, business, or system capabilities that phases deliver. They have no lifecycle status. Never store coding standards, best practices, formatting rules, verification process, or agent behavior in Requirements; store those only in Project Guidelines. Nested Requirement macro-tasks retain their own implementation status.

- Read Project Guidelines on planner load when present and whenever lifecycle `nextActions` says it is missing or stale.
- Update it only through `project_guidelines_update`, `planner-project-guidelines-update`, or Pi `/planner project guidelines`.
- Explicit planner load automatically and atomically deduplicates legacy `globalRules`, textual `workflowRules`, and project `decisions` into canonical Project Guidelines and Accepted Decisions before recap/context delivery. Ordinary entity reads remain non-mutating. `project_context_migrate` and `planner-project-context-migrate` remain manual preview/recovery diagnostics; repeated applications are idempotent.
- The Web UI may display the section for the human supervisor, but guideline-read enforcement applies to agents.

## Task execution

- Create rich feature, phase, and task descriptions with current state, concrete goals, relevant systems, file/symbol references, behaviors to preserve, and edge cases.
- Use one task with checklist items for implementation steps. Do not create child tasks merely to scatter the same execution context.
- Add, remove, and toggle checklist items granularly. Do not encode completion by adding `DONE` to checklist titles.
- Use `task_pause` / `planner-task-pause` with the reason, work underway, exact resume location, and actionable resume instructions.
- Use `task_switch` / `planner-task-switch` for temporary detours. It atomically checkpoints the source and preserves a LIFO return target.
- A temporary task completion can emit `RESUME REQUIRED`. Resume the preserved source or deliberately switch again; do not silently abandon it.
- Complete a task only after implementation and verification. Supply durable completion evidence, files touched, decisions, remaining or unverified work, and updated code references.
- Status changes to `blocked`, `canceled`, `rejected`, `deferred`, `waiting`, or back to `planned` require a substantive motivation on agent surfaces.

## Mutation integrity and oversized descriptions

Every mutation is success-sensitive:

- Treat `isError`, `updated: false`, `started: false`, or a typed failure code as non-success even if text is also returned.
- Read back important mutations. Confirm the intended fields actually persisted before reporting success.
- If an update/discuss operation returns `DESCRIPTION_MARKDOWN_FALLBACK_REQUIRED`, create the suggested committed Markdown file under `.planner/docs/`, retry with a concise inline summary plus `descriptionRef`, then read back the entity and verify the reference.
- If it returns `NO_MUTABLE_FIELDS_RECEIVED`, do not claim an update occurred.
- Destructive deletes require explicit user confirmation and a data-loss warning.

## Handoff protocol

Handoffs are phase-scoped resume capsules, not locks. They transfer only the context a cold agent needs to continue safely. Every canonical terminal phase outcome archives its active handoff automatically; terminal phases cannot receive a new handoff.

Before writing:

1. Resolve one exact phase reference and obtain user confirmation when required.
2. Run `handoff_prepare` / `planner-handoff-prepare` for that exact phase. Review its bounded, priority-ordered `phaseWorkMap`, then reread the canonical phase and every relevant sibling task full view before describing remaining work; preserve existing capability ownership rather than proposing duplicates.
3. **Before generating prose**, read the returned `requiredHumanInputs` and supply them structurally (including `reason`). The returned `draftTemplate` is guidance, not a form: headings are optional and concise free-form resume prose is accepted. Never write `Created at`, `Updated at`, or `Reason` into Markdown: the planner generates those fields before persistence.
4. Reconcile every still-relevant detail from an existing handoff; do not append a competing handoff.
5. Do not build or copy a completeness audit into the Markdown. Legacy `completenessAudit` input is optional; the planner records derived evidence as structured metadata.
6. Do not build or copy a cold-start inventory or five source reviews into the Markdown. Legacy `coldStartInventory`, `sourceReviews`, and `omissionsFound` inputs are optional; the planner derives omitted evidence from persisted state. Include concrete files/symbols, verification, constraints, blockers, and next steps only when they are needed to resume.

The resume-critical content is: exact focus and resume point; current/partial state; decisions and constraints; relevant files/symbols; verification and pending checks; blockers; and ordered next actions. Omit categories that have no resume impact.

Write the canonical handoff as a compact resume capsule targeting at most 8,000 inline characters (24,000 remains only as an absolute compatibility ceiling). Keep inline only the exact focus, current/partial state, decisions or constraints, relevant verification, blockers, and ordered resume steps. Put extended detail in `.planner/docs/` only when the next agent genuinely needs it; externalization is automatic and does not require a manual inventory.

Then call `handoff_write` / `planner-handoff-write` once with the preparation token, structured `reason`, compact capsule, optional supporting-document manifest, and reconciled task/phase/feature context. Missing preflight/reason, unresolved placeholders, invalid documents, or failed persistence read-back are typed failures and must never be reported as success.

A successful write persists only a **handoff candidate** and returns `resumeReady: false`. Immediately call `handoff_show` / `planner-handoff-show` with the exact phase reference and read the persisted capsule. If the capsule omits resume-critical context, rewrite it; otherwise call `handoff_verify` / `planner-handoff-verify` with the content hash. Source reviews and omission lists are optional legacy evidence; the planner derives them when omitted. Only a successful verification result with `resumeReady: true` authorizes telling the user that the handoff is resume-ready.

`handoff_list` is a compact paginated index of active handoffs and exposes whether each is resume-ready. Use `handoff_show` for one bounded active body and its metadata; after phase completion/rejection/cancellation, the same exact phase-scoped show call returns the latest terminal archive so its closeout and `.planner/docs/` references remain discoverable. Clear/archive only after explicit intent or when phase completion makes the handoff non-operational.

## Hierarchical description freshness

Task description or descriptionRef changes can make the owning phase and feature prose stale; phase description changes can make the owning feature stale. Use `description_freshness` / `planner-description-freshness` to read the deterministic, non-mutating leaf-to-root reconciliation preview. Read the cited child and parent full views, then explicitly update only parent prose that is actually obsolete. Never silently copy child text into a parent or claim the hierarchy is fresh from a generic entity timestamp alone; the preview uses description-specific revisions and returns exact stale parent refs.

## Ideas Inbox and promotion

Ideas are independent inbox entries (`I001`, never feature/phase/task children) and do not affect work rollups. Use `planner-idea-list` / `idea_list` to discover and `*-show/create/update/delete` for CRUD. For promotion, first call `planner-idea-promotion-begin` / `idea_promotion_begin`; it loads the project-local `grill-me` instructions only for that discussion. Follow its one-question-at-a-time interview, obtain explicit target confirmation, create the agreed target through normal creation tools, then call `*-promotion-finalize` with `discussionCompleted=true` and `confirmed=true`. Begin never persists a target or promotion; finalize rejects missing confirmation or an unresolved target.

## Pi `/planner` command routing

Supported interactive command paths:

### Core and project

- `/planner init`
- `/planner show`
- `/planner version`
- `/planner repair`
- `/planner cleanup-orphans`
- `/planner load`
- `/planner stop` or `/planner disable`
- `/planner project discuss`
- `/planner project language`
- `/planner project guidelines`
- `/planner project migrate-context`

### Ideas Inbox

- `/planner idea list`
- `/planner idea add [title]`
- `/planner idea show <I00x>`
- `/planner idea update <I00x>`
- `/planner idea delete <I00x>`
- `/planner idea promote <I00x>`

### Features and phases

- `/planner feature list`
- `/planner feature add`
- `/planner feature show <F00x>`
- `/planner feature discuss <F00x>`
- `/planner feature update <F00x>`
- `/planner feature delete <F00x>`
- `/planner phase list [F00x]`
- `/planner phase add <F00x>`
- `/planner phase show <P00x(F00x)>`
- `/planner phase discuss <P00x(F00x)>`
- `/planner phase update <P00x(F00x)>`
- `/planner phase delete <P00x(F00x)>`

### Tasks and handoffs

- `/planner task list <P00x(F00x)>`
- `/planner task add <P00x(F00x)>`
- `/planner task show <P00x(F00x)/T00x>`
- `/planner task discuss <P00x(F00x)/T00x>`
- `/planner task update <P00x(F00x)/T00x>`
- `/planner task delete <P00x(F00x)/T00x>`
- `/planner task start <P00x(F00x)/T00x>`
- `/planner task complete <P00x(F00x)/T00x>`
- `/planner task checklist-add <task> <title>`
- `/planner task checklist-remove <task> <C{n}|id|title>`
- `/planner task checklist-toggle <task> <C{n}|id|title> [on|off]`
- `/planner handoff list`
- `/planner handoff prepare`
- `/planner handoff show <P00x(F00x)>`
- `/planner handoff write <P00x(F00x)>`
- `/planner handoff clear <P00x(F00x)>`

`handoff_verify` is an agent tool rather than an interactive command; call it only after `handoff_show` completes the separate persisted read-back.

Pause, switch, deviation, recommendation, requirement, and decision operations are available through the registered Pi tools below rather than every interactive `/planner` path.

### Dashboard, export, and guard

- `/planner web start`
- `/planner web stop`
- `/planner web status`
- `/planner export`
- `/planner export-full`
- `/planner bypass [minutes]`
- `/planner clear-bypass`

## MCP tool inventory

The MCP adapter publishes these tools:

- Core: `planner-version`, `planner-init`, `planner-show`, `planner-description-freshness`, `planner-repair`, `planner-cleanup-orphan-phases`, `planner-export`, `planner-authorize-bypass`, `planner-clear-bypass`, `planner-load`, `planner-disable`, `planner-web`.
- Ideas: `planner-idea-list`, `planner-idea-show`, `planner-idea-create`, `planner-idea-update`, `planner-idea-delete`, `planner-idea-promotion-begin`, `planner-idea-promotion-finalize`.
- Project: `planner-project-language`, `planner-project-discuss`, `planner-project-guidelines-show`, `planner-project-guidelines-update`, `planner-project-context-migrate`, `planner-accepted-decision-create`, `planner-accepted-decision-update`, `planner-accepted-decision-delete`, `planner-requirement-list`, `planner-requirement-create`, `planner-requirement-update`, `planner-requirement-delete`.
- Features: `planner-feature-list`, `planner-feature-add`, `planner-feature-show`, `planner-feature-discuss`, `planner-feature-update`, `planner-feature-delete`.
- Phases: `planner-phase-list`, `planner-phase-add`, `planner-phase-show`, `planner-phase-discuss`, `planner-phase-update`, `planner-phase-delete`.
- Tasks: `planner-task-list`, `planner-task-add`, `planner-task-show`, `planner-task-discuss`, `planner-task-update`, `planner-task-dependency-add`, `planner-task-dependency-delete`, `planner-task-delete`, `planner-task-recommend`, `planner-task-deviation`, `planner-task-pause`, `planner-task-switch`, `planner-task-start`, `planner-task-reopen`, `planner-task-complete`, `planner-task-checklist-toggle`, `planner-task-checklist-add`, `planner-task-checklist-remove`.
- Handoffs: `planner-handoff-list`, `planner-handoff-show`, `planner-handoff-prepare`, `planner-handoff-write`, `planner-handoff-verify`, `planner-handoff-clear`.

## Pi tool inventory

The Pi adapter registers these tools:

- Ideas: `idea_list`, `idea_show`, `idea_create`, `idea_update`, `idea_delete`, `idea_promotion_begin`, `idea_promotion_finalize`.
- Project and requirements: `project_set_language_preferences`, `project_update`, `project_guidelines_show`, `project_guidelines_update`, `project_context_migrate`, `accepted_decision_create`, `accepted_decision_update`, `accepted_decision_delete`, `requirement_list`, `requirement_create`, `requirement_update`, `requirement_delete`.
- Plan: `plan_init`, `plan_get`, `description_freshness`, `plan_render`, `plan_repair`, `plan_cleanup_orphan_phases`, `plan_authorize_bypass`, `plan_clear_bypass`.
- Features: `feature_list`, `feature_get`, `feature_create`, `feature_discuss`, `feature_update`, `feature_delete`.
- Phases and decisions: `phase_list`, `phase_get`, `phase_create`, `phase_discuss`, `phase_update`, `phase_delete`, `decision_record`.
- Tasks: `task_list`, `task_get`, `task_create`, `task_update`, `task_dependency_add`, `task_dependency_delete`, `task_delete`, `task_recommend`, `task_deviation`, `task_pause`, `task_switch`, `task_start`, `task_reopen`, `task_complete`, `task_checklist_toggle`, `task_checklist_add`, `task_checklist_remove`.
- Handoffs: `handoff_list`, `handoff_show`, `handoff_prepare`, `handoff_write`, `handoff_verify`, `handoff_clear`.
- Dashboard and lifecycle: `planner-web`, `planner-load`, `planner-stop`.
- Deprecated compatibility aliases: `plan_get_handoff`, `plan_write_handoff`, `plan_delete_handoff`. Prefer the entity-scoped handoff tools.

## Managed-copy policy

Agent Plan owns only uncustomized managed copies of `.planner/SKILL.md`:

- New planners receive a deterministic, timestamp-free managed copy.
- When the canonical skill changes, an unmodified managed copy upgrades automatically on explicit planner load.
- If project members customize the copy, Agent Plan preserves it and reports actionable drift instead of overwriting it.
- Resolve drift deliberately by reconciling project customizations with the current canonical guide; never discard custom instructions silently.
