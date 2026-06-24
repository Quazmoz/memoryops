use axum::{extract::Path, extract::Query, extract::State, Extension, Json};
use chrono::{DateTime, Utc};
use common::{
    audit::{write_audit, AuditEvent, RequestContext},
    auth::AuthContext,
    error::AppResult,
    models::AuditAction,
    AppError, AppState,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres};
use uuid::Uuid;

const MAX_RESOURCE_NAME_LEN: usize = 64;
const MAX_RESOURCE_TITLE_LEN: usize = 120;
const MAX_RESOURCE_DESCRIPTION_LEN: usize = 500;
const MAX_RESOURCE_BODY_LEN: usize = 100_000;
const MAX_RESOURCE_CONTENT_LEN: usize = 120_000;
const MAX_CHANGE_NOTE_LEN: usize = 500;

const AGENT_RESOURCE_COLUMNS: &str = "id, workspace_id, kind, assistant, name, filename, title, \
     description, body, content, metadata, version, created_at, updated_at";

const AGENT_RESOURCE_VERSION_COLUMNS: &str = "id, resource_id, workspace_id, kind, assistant, \
     name, filename, title, description, body, content, metadata, version, change_note, \
     created_by, created_at";

#[derive(Debug, Deserialize)]
pub struct AgentResourceListQuery {
    pub kind: Option<String>,
    pub assistant: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateAgentResourceRequest {
    pub kind: String,
    pub assistant: Option<String>,
    pub name: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub content: Option<String>,
    pub metadata: Option<Value>,
    pub change_note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateAgentResourceRequest {
    pub title: Option<String>,
    pub description: Option<String>,
    pub body: Option<String>,
    pub content: Option<String>,
    pub metadata: Option<Value>,
    pub change_note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RollbackAgentResourceRequest {
    pub change_note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentResourceKind {
    Skill,
    Agent,
    Prompt,
    Instruction,
}

impl AgentResourceKind {
    fn parse(value: &str) -> AppResult<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "skill" => Ok(Self::Skill),
            "agent" => Ok(Self::Agent),
            "prompt" => Ok(Self::Prompt),
            "instruction" => Ok(Self::Instruction),
            _ => Err(AppError::Validation(
                "Resource kind must be one of skill, agent, prompt, or instruction".to_owned(),
            )),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Skill => "skill",
            Self::Agent => "agent",
            Self::Prompt => "prompt",
            Self::Instruction => "instruction",
        }
    }

    fn title_label(self) -> &'static str {
        match self {
            Self::Skill => "Skill",
            Self::Agent => "Agent",
            Self::Prompt => "Prompt",
            Self::Instruction => "Instruction",
        }
    }
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AgentResource {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub kind: String,
    pub assistant: String,
    pub name: String,
    pub filename: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub content: String,
    pub metadata: Value,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AgentResourceSummary {
    pub id: Uuid,
    pub workspace_id: Uuid,
    pub kind: String,
    pub assistant: String,
    pub name: String,
    pub filename: String,
    pub title: String,
    pub description: String,
    pub metadata: Value,
    pub version: i32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct AgentResourceVersion {
    pub id: Uuid,
    pub resource_id: Uuid,
    pub workspace_id: Uuid,
    pub kind: String,
    pub assistant: String,
    pub name: String,
    pub filename: String,
    pub title: String,
    pub description: String,
    pub body: String,
    pub content: String,
    pub metadata: Value,
    pub version: i32,
    pub change_note: Option<String>,
    pub created_by: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, sqlx::FromRow)]
struct ResourceWriteState {
    title: String,
    description: String,
    body: String,
    content: String,
    metadata: Value,
}

#[derive(Clone, Copy)]
pub struct SkillResourceInput<'a> {
    pub assistant: &'a str,
    pub name: &'a str,
    pub filename: &'a str,
    pub title: &'a str,
    pub description: &'a str,
    pub instructions: &'a str,
    pub content: &'a str,
}

#[derive(Clone, Copy)]
struct DefaultAgentResourceInput {
    kind: AgentResourceKind,
    assistant: &'static str,
    name: &'static str,
    title: &'static str,
    description: &'static str,
    body: &'static str,
}

const DEFAULT_AGENT_RESOURCE_KINDS: [AgentResourceKind; 4] = [
    AgentResourceKind::Skill,
    AgentResourceKind::Agent,
    AgentResourceKind::Prompt,
    AgentResourceKind::Instruction,
];

const DEFAULT_AGENT_RESOURCES: &[DefaultAgentResourceInput] = &[
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Instruction,
        assistant: "generic",
        name: "coding_agent_memory_rules",
        title: "Coding Agent Memory Rules",
        description:
            "Rules for when coding agents should retrieve, store, or ignore MemoryOps context.",
        body: r#"## Trigger

Apply this instruction at the start and end of any substantial coding, debugging, release, or incident task.

## Execution Steps

1. Retrieve context before making project-specific decisions. Search for architecture rules, prior incidents, deployment constraints, and user preferences.
2. Use retrieved memories as supporting evidence. Prefer recent, scoped, and corroborated memories over broad or stale memories.
3. Store durable outcomes only after the task is complete: decisions, resolved root causes, migration notes, and stable workflow rules.
4. Use observation ingestion for raw logs or notes that still need later classification.

## Failure Handling

- If MemoryOps credentials or tools are unavailable, continue with local repo inspection and mention that memory context was unavailable.
- If retrieved memories conflict, pause before acting on either one and apply the contradiction-management instruction.

## Output Expectations

When memory materially influenced the work, mention the relevant finding briefly in the handoff."#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Prompt,
        assistant: "generic",
        name: "memory_retrieval_prompt",
        title: "Memory Retrieval Prompt",
        description:
            "Reusable prompt for retrieving focused workspace context before an agent acts.",
        body: r#"## Prompt

Retrieve MemoryOps context for the task below. Search for durable facts, architectural decisions, known pitfalls, relevant incidents, and user preferences.

## Inputs

- Task: {{task}}
- Repository or subsystem: {{repo_or_subsystem}}
- Time sensitivity: {{time_sensitivity}}

## Retrieval Guidance

1. Start with a narrow query using concrete identifiers from the task.
2. Run a second broader query for related decisions or incidents.
3. Prefer semantic memories for stable rules and episodic memories for recent operational evidence.
4. Do not treat a single stale memory as authoritative if newer context exists.

## Output

Return the top findings as concise bullets with source hints, confidence, and any contradictions or missing context."#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Prompt,
        assistant: "generic",
        name: "memory_storage_observation_prompt",
        title: "Memory Storage Observation Prompt",
        description:
            "Reusable prompt for deciding whether to store a memory or submit an observation.",
        body: r#"## Prompt

Decide whether the following information should be saved to MemoryOps.

## Inputs

- Candidate information: {{candidate_information}}
- Task outcome: {{task_outcome}}
- Scope: {{workspace_or_repo_scope}}

## Decision Rules

1. Store a memory when the information is durable, reusable, and likely to help future agents or operators.
2. Submit an observation when the content is raw evidence, logs, symptoms, or a partial hypothesis that needs processing.
3. Do not store transient task steps, private chain-of-thought, secrets, credentials, or noisy intermediate output.
4. Include tags for subsystem, tool, incident, dependency, or policy when available.

## Output

Return one of: `store`, `observe`, or `skip`, followed by the exact concise content and tags to send."#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Instruction,
        assistant: "generic",
        name: "contradiction_management",
        title: "Contradiction Management",
        description:
            "Guidance for handling conflicting memories without amplifying stale or unsafe context.",
        body: r#"## Trigger

Use this instruction when retrieved memories disagree, a user disputes a memory, or a new observation invalidates older context.

## Execution Steps

1. Identify the conflicting claims and their scopes: workspace, repository, service, branch, user, and timestamp.
2. Prefer the most recent validated source only when it is in the same scope and directly addresses the conflict.
3. If both claims can be true in different scopes, keep both and record the distinction.
4. If one claim is stale or incorrect, resolve or flag the contradiction rather than silently ignoring it.
5. When uncertain, ask for confirmation before storing a replacement memory.

## Failure Handling

- If contradiction tools are unavailable, summarize the conflict and avoid relying on either claim as fact.
- Never delete or overwrite memory solely because it is inconvenient; require evidence or user confirmation.

## Output Expectations

Explain which claim is being used, why, and whether follow-up resolution is needed."#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Agent,
        assistant: "generic",
        name: "production_code_review",
        title: "Production Code Review Agent",
        description:
            "Agent profile for risk-first production code review with MemoryOps context retrieval.",
        body: r#"## Role

Act as a production code-review agent. Prioritize correctness, security, data safety, migrations, compatibility, and missing tests over style preferences.

## Operating Rules

1. Retrieve MemoryOps context for the subsystem, prior regressions, and release constraints before reviewing.
2. Read the diff and the surrounding code paths. Do not infer behavior from filenames alone.
3. Lead with actionable findings ordered by severity. Include file and line references.
4. Call out missing tests only when they protect a realistic failure mode.
5. Do not approve broad rewrites unless they are necessary for the stated change.

## Failure Handling

- If the diff or memory context is incomplete, state the gap and review the available code.
- If a finding depends on an assumption, label it as an assumption.

## Output

Return findings first, then open questions, then a short residual-risk summary."#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Instruction,
        assistant: "generic",
        name: "token_budget_policy",
        title: "Token Budget Policy",
        description:
            "Reusable policy for controlling agent verbosity without losing technical accuracy.",
        body: r#"## Purpose

Use this instruction when an agent should adapt response length for MemoryOps-assisted work while preserving exact technical content.

## Compression Modes

- normal: Default helpful response with enough explanation for the task.
- compact: Shorter response that still explains relevant reasoning and tradeoffs.
- dense: Bullets and exact technical actions only.
- ultra: Minimum viable technical answer; use only when explicitly requested.

## Protected Content

- Do not rewrite or compress code blocks destructively.
- Do not alter CLI flags, enum values, JSON fields, API routes, file paths, package names, versions, hashes, placeholders, or quoted errors.
- Do not drop `unknown`, `untested`, `not verified`, or `assumption` labels.
- Do not drop warnings related to secrets, security, privacy, data loss, migrations, billing, or production risk.

## Output Expectations

- Prefer direct actions, changed files, tests run, and residual risk.
- Remove filler, broad restatement, obvious caveats, and repeated context.
- Keep MemoryOps storage candidates limited to durable decisions, root causes, migration notes, architecture rules, stable project preferences, and reusable workflow rules."#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Prompt,
        assistant: "generic",
        name: "compact_context_handoff",
        title: "Compact Context Handoff",
        description:
            "Prompt for compressing retrieved MemoryOps context before handing it to another agent.",
        body: r#"## Prompt

Compress MemoryOps retrieved context for handoff to another agent. Keep only context that changes the next action.

## Inputs

- Task: {{task}}
- Retrieved memories: {{retrieved_memories}}
- Repository/subsystem: {{repository_or_subsystem}}
- Token budget: {{token_budget}}
- Time horizon: {{time_horizon}}
- Known conflicts: {{known_conflicts}}

## Rules

1. Deduplicate repeated memories.
2. Preserve memory/source IDs when present.
3. Preserve contradictions instead of silently choosing one.
4. Prefer durable project facts over transient logs.
5. Keep only context that changes the next action.

## Output

Facts:
- 

Decisions:
- 

Constraints:
- 

Conflicts:
- 

Next action:
- "#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Agent,
        assistant: "generic",
        name: "token_efficient_reviewer",
        title: "Token Efficient Reviewer",
        description: "Risk-first code review agent profile optimized for very low-token output.",
        body: r#"## Role

Act as a risk-first code review agent that uses very few tokens.

## Operating Rules

1. Retrieve MemoryOps context first when available.
2. Lead with blockers only.
3. Then correctness bugs.
4. Then security and data risks.
5. Then missing tests.
6. Avoid praise and generic explanation.
7. Label assumptions and unverified claims.

## Output

BLOCKER:
- 

BUG:
- 

RISK:
- 

TEST GAP:
- 

PATCH TARGET:
- 

RESIDUAL RISK:
- "#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Agent,
        assistant: "generic",
        name: "token_efficient_builder",
        title: "Token Efficient Builder",
        description: "Implementation agent profile optimized for small, low-token coding loops.",
        body: r#"## Role

Act as an implementation agent optimized for low-token coding loops.

## Operating Rules

1. Plan only when needed.
2. Make small, scoped changes.
3. Retrieve and apply MemoryOps context when it can affect implementation choices.
4. Avoid long summaries, repeated context, and obvious explanations.
5. Store only durable MemoryOps outcomes: decisions, root causes, migration notes, architecture rules, stable project preferences, and reusable workflow rules.

## Output

Report only:
- files changed
- key implementation decisions
- tests run
- unresolved risks
- MemoryOps observations worth storing"#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Agent,
        assistant: "generic",
        name: "token_restriction_light",
        title: "Light Token Restriction Agent",
        description:
            "Agent profile for modest token reduction with no intentional performance loss.",
        body: r#"## Role

Act as a general-purpose agent using light token restriction. Preserve normal task performance and reduce only avoidable verbosity.

## Token Measures

1. Remove greetings, filler, repeated user restatement, and obvious explanations.
2. Use short paragraphs or bullets, but keep enough context for user decisions.
3. Summarize tool output instead of pasting logs unless exact lines matter.
4. Reference files, commands, API fields, versions, and errors exactly.
5. Keep safety, security, privacy, data-loss, migration, billing, and production-risk warnings.

## Performance Guardrails

- Do not skip discovery, verification, or tests to save tokens.
- Do not compress code, commands, config, or error text in a way that changes meaning.
- Ask a concise question when missing information would materially change the result.

## Output

Return:
- answer or change made
- key evidence
- tests or checks
- residual risk"#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Agent,
        assistant: "generic",
        name: "token_restriction_medium",
        title: "Medium Token Restriction Agent",
        description:
            "Agent profile for substantial token reduction with only slight performance tradeoff.",
        body: r#"## Role

Act as a task-focused agent using medium token restriction. Reduce explanation depth while preserving correctness, safety, and implementation quality.

## Token Measures

1. Plan only for multi-step or risky tasks.
2. Prefer terse bullets over narrative.
3. Collapse routine findings into one-line summaries.
4. Include only changed files, root cause, implementation decision, verification, and risk.
5. Omit praise, generic caveats, alternate approaches not being used, and low-value history.
6. Store or suggest MemoryOps content only for durable outcomes.

## Performance Guardrails

- Slight performance tradeoff is acceptable only in explanation detail, not in code quality or safety.
- Do not skip relevant file inspection, dependency checks, or tests solely to save tokens.
- Preserve exact commands, code, file paths, API routes, config keys, versions, hashes, and quoted errors.
- Label `unknown`, `untested`, `not verified`, and `assumption` when applicable.

## Output

Use this shape:
- DONE:
- DECISION:
- CHECKS:
- RISK:
- NEXT:"#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Agent,
        assistant: "generic",
        name: "token_restriction_heavy",
        title: "Heavy Token Restriction Agent",
        description:
            "Agent profile for aggressive token reduction while preserving safety-critical accuracy.",
        body: r#"## Role

Act as an agent under heavy token restriction. Optimize for minimum useful tokens while preserving safety-critical accuracy and task completion.

## Token Measures

1. Use dense bullets and fragments when clear.
2. Do not include background unless it changes the next action.
3. Report tool results as pass/fail plus the decisive line only.
4. Prefer patch target, exact command, exact file, and residual risk over explanation.
5. Avoid plans unless the task is risky, ambiguous, or multi-system.
6. Use MemoryOps only for context that can change the work or durable outcomes worth storing.

## Performance Guardrails

- Small performance tradeoff is acceptable for convenience, explanation depth, and optional alternatives.
- No tradeoff is allowed for security, privacy, data loss, migrations, billing, production risk, destructive actions, or compatibility.
- Do not omit blockers, failed checks, assumptions, unknowns, rollback needs, or user approvals.
- Preserve exact commands, code, file paths, API routes, config keys, versions, hashes, placeholders, and quoted errors.

## Output

Use this shape:
- RESULT:
- FILES:
- CHECKS:
- BLOCKERS:
- RISK:
- STORE:"#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Instruction,
        assistant: "generic",
        name: "token_efficiency_routing",
        title: "Token Efficiency Routing",
        description:
            "Instruction for choosing light, medium, or heavy token restriction by task risk.",
        body: r#"## Purpose

Choose the right token restriction level before an agent starts work.

## Routing Rules

- Use light for ambiguous, design-heavy, onboarding, user-facing, or high-empathy tasks.
- Use medium for routine coding, debugging, documentation, PR review, and operational tasks.
- Use heavy for status updates, known fixes, log triage, repeated checks, and explicit terse-mode requests.
- De-escalate to light when missing context, safety risk, data loss, migrations, billing, production impact, or user approval is involved.
- Escalate to heavy only after the task shape is clear.

## Guardrails

- Token savings may reduce explanation, not correctness.
- Never skip necessary code inspection, verification, or safety caveats to fit a token budget.
- Preserve exact commands, paths, fields, versions, hashes, and quoted errors.
- Label assumptions, unknowns, and unverified results.

## Output

Return:
- mode
- reason
- protected content
- next action"#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Instruction,
        assistant: "generic",
        name: "exactness_preservation",
        title: "Exactness Preservation",
        description:
            "Instruction for preserving technical identifiers while compressing agent output.",
        body: r#"## Purpose

Compress prose while preserving exact technical content.

## Protected Content

- Commands and flags
- File paths and line references
- API routes, JSON fields, enum values, config keys, environment variables, and package names
- Versions, SHAs, hashes, IDs, ports, regions, timestamps, and placeholders
- Error messages, warnings, stack frames, migration names, and rollback steps

## Rules

1. Never paraphrase protected content when precision matters.
2. If a long protected value is too large, quote the decisive segment and say what was omitted.
3. Keep `unknown`, `untested`, `not verified`, `assumption`, and `requires approval` labels.
4. Do not compress code blocks destructively.
5. Prefer one exact reference over a vague summary.

## Output Expectations

- Use prose compression around protected content.
- Keep enough exact detail for another agent or operator to act safely."#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Instruction,
        assistant: "generic",
        name: "tool_output_compression",
        title: "Tool Output Compression",
        description:
            "Instruction for summarizing command, log, and test output with fewer tokens.",
        body: r#"## Purpose

Compress tool output while preserving actionability.

## Rules

1. Report pass/fail first.
2. Include the command name when relevant.
3. Keep the decisive line, error code, failing test, file path, or stack frame.
4. Summarize repeated log lines by count or pattern.
5. Omit routine compile progress, dependency noise, and successful boilerplate.
6. Do not omit warnings about secrets, security, privacy, data loss, migrations, billing, or production risk.

## Output

Use:
- COMMAND:
- RESULT:
- DECISIVE OUTPUT:
- NEXT:"#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Instruction,
        assistant: "generic",
        name: "memoryops_token_hygiene",
        title: "MemoryOps Token Hygiene",
        description:
            "Instruction for retrieving and storing MemoryOps context without token waste.",
        body: r#"## Purpose

Use MemoryOps efficiently without storing noisy or sensitive content.

## Retrieval Rules

- Start with narrow identifiers from the task.
- Broaden only when the first result set lacks durable context.
- Deduplicate repeated memories before presenting them.
- Preserve contradictions and source IDs when present.
- Prefer durable project facts over transient logs.

## Storage Rules

- Store only stable decisions, root causes, migration notes, architecture rules, project preferences, and reusable workflow rules.
- Do not store secrets, credentials, private reasoning, scratchpad notes, raw tool noise, or temporary status.
- Keep memory candidates short, factual, scoped, and tagged.

## Output

When MemoryOps matters, report:
- retrieved fact
- source or confidence
- storage candidate
- skip reason"#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Prompt,
        assistant: "generic",
        name: "token_budget_selector",
        title: "Token Budget Selector",
        description:
            "Prompt for selecting an appropriate token restriction mode for an agent task.",
        body: r#"## Prompt

Select the token mode for this task.

## Inputs

- Task: {{task}}
- User preference: {{user_preference}}
- Risk level: {{risk_level}}
- Repository/subsystem: {{repository_or_subsystem}}
- Available context: {{available_context}}
- Deadline or budget: {{deadline_or_budget}}

## Rules

- Choose light, medium, heavy, dense, or ultra.
- Prefer lower restriction for unclear or risky work.
- Prefer higher restriction for status, repeated checks, and known procedures.
- Identify protected content that must remain exact.

## Output

Mode:

Reason:

Protected content:

First action:"#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Prompt,
        assistant: "generic",
        name: "compact_patch_plan",
        title: "Compact Patch Plan",
        description:
            "Prompt for producing a terse implementation plan focused on files and verification.",
        body: r#"## Prompt

Create a compact patch plan.

## Inputs

- Task: {{task}}
- Relevant files: {{relevant_files}}
- Retrieved context: {{retrieved_context}}
- Constraints: {{constraints}}
- Token mode: {{token_mode}}

## Rules

- Plan only the minimum safe sequence.
- Include exact files and commands when known.
- Separate assumptions from facts.
- Include rollback or migration notes only when relevant.

## Output

FILES:
- 

STEPS:
- 

CHECKS:
- 

RISKS:
- "#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Prompt,
        assistant: "generic",
        name: "compact_review_findings",
        title: "Compact Review Findings",
        description:
            "Prompt for formatting code review findings with very low token overhead.",
        body: r#"## Prompt

Format code review findings for a low-token handoff.

## Inputs

- Diff or files: {{diff_or_files}}
- Retrieved context: {{retrieved_context}}
- Review scope: {{review_scope}}
- Risk tolerance: {{risk_tolerance}}

## Rules

- Findings first.
- No praise or generic explanation.
- Include file and line when available.
- Include only realistic bugs, security/data risks, and test gaps.
- Label assumptions.

## Output

BLOCKER:
- 

BUG:
- 

RISK:
- 

TEST GAP:
- 

PATCH TARGET:
- "#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Prompt,
        assistant: "generic",
        name: "compact_debug_report",
        title: "Compact Debug Report",
        description:
            "Prompt for compressing debugging evidence into root cause, fix, and verification.",
        body: r#"## Prompt

Compress debugging work into a concise technical report.

## Inputs

- Symptom: {{symptom}}
- Evidence: {{evidence}}
- Logs/errors: {{logs_or_errors}}
- Changes tested: {{changes_tested}}
- Remaining unknowns: {{remaining_unknowns}}

## Rules

- Preserve exact error text and failing command names.
- Prefer root cause and next action over chronology.
- Keep only evidence that changes diagnosis or fix.
- Label unverified hypotheses.

## Output

SYMPTOM:

ROOT CAUSE:

FIX:

CHECKS:

UNKNOWN:"#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Prompt,
        assistant: "generic",
        name: "compact_final_handoff",
        title: "Compact Final Handoff",
        description:
            "Prompt for producing terse final task summaries with checks and residual risk.",
        body: r#"## Prompt

Write a compact final handoff for completed agent work.

## Inputs

- Task: {{task}}
- Files changed: {{files_changed}}
- Decisions: {{decisions}}
- Checks run: {{checks_run}}
- Failures or skipped checks: {{failures_or_skipped_checks}}
- Residual risk: {{residual_risk}}
- MemoryOps candidate: {{memoryops_candidate}}

## Rules

- Do not repeat the full task.
- Include exact check commands and pass/fail state.
- Mention skipped checks only when meaningful.
- Include MemoryOps storage candidate only when durable.

## Output

Changed:

Checks:

Risk:

MemoryOps:"#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Agent,
        assistant: "generic",
        name: "token_efficient_planner",
        title: "Token Efficient Planner",
        description:
            "Agent profile for creating minimal safe plans before coding or operations work.",
        body: r#"## Role

Act as a planning agent that spends tokens only on decisions that affect execution.

## Operating Rules

1. Retrieve MemoryOps context when project history can affect the plan.
2. Identify the smallest safe work sequence.
3. Omit background and obvious setup.
4. Include exact files, commands, dependencies, approvals, and rollback points when known.
5. Escalate verbosity for destructive, production, migration, billing, or security-sensitive work.

## Output

MODE:
- 

FILES:
- 

STEPS:
- 

CHECKS:
- 

RISKS:
- "#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Agent,
        assistant: "generic",
        name: "token_efficient_debugger",
        title: "Token Efficient Debugger",
        description:
            "Agent profile for concise root-cause debugging with exact evidence preservation.",
        body: r#"## Role

Act as a debugger optimized for low-token diagnosis without skipping evidence.

## Operating Rules

1. Start from the failing command, symptom, or error.
2. Retrieve MemoryOps context for prior incidents or subsystem rules when available.
3. Inspect the nearest code path before broad search.
4. Preserve exact errors, versions, file paths, config keys, and commands.
5. Report hypotheses only when they change the next test.
6. Stop once root cause and verification are clear.

## Output

FAIL:
- 

CAUSE:
- 

FIX:
- 

VERIFY:
- 

RISK:
- "#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Agent,
        assistant: "generic",
        name: "token_efficient_devops",
        title: "Token Efficient DevOps Agent",
        description:
            "Agent profile for terse infrastructure, deployment, and operational workflows.",
        body: r#"## Role

Act as a DevOps agent using concise output while preserving operational safety.

## Operating Rules

1. Retrieve MemoryOps context for deployment constraints, incidents, and environment rules.
2. Preserve exact cluster, namespace, resource, image, version, region, and command values.
3. Summarize logs by decisive lines and repeated patterns.
4. Never omit rollback, data-loss, security, production, or approval caveats.
5. Prefer current state, action, verification, and rollback over narrative.

## Output

STATE:
- 

ACTION:
- 

VERIFY:
- 

ROLLBACK:
- 

RISK:
- "#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Agent,
        assistant: "generic",
        name: "token_efficient_test_writer",
        title: "Token Efficient Test Writer",
        description:
            "Agent profile for adding focused tests with concise rationale and verification.",
        body: r#"## Role

Act as a test-writing agent that minimizes explanation while protecting behavior.

## Operating Rules

1. Identify the behavior, regression, or invariant under test.
2. Add the smallest meaningful test that would fail without the fix.
3. Reuse existing test helpers and local style.
4. Avoid broad test rewrites unless required.
5. Report only coverage target, files changed, command run, and remaining gap.

## Output

TARGET:
- 

FILES:
- 

CHECKS:
- 

GAP:
- "#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Agent,
        assistant: "generic",
        name: "token_efficient_context_curator",
        title: "Token Efficient Context Curator",
        description:
            "Agent profile for filtering MemoryOps context down to action-changing facts.",
        body: r#"## Role

Act as a context curator that compresses retrieved MemoryOps content for another agent.

## Operating Rules

1. Deduplicate repeated memories.
2. Preserve source IDs, scope, timestamps, and contradictions when present.
3. Prefer durable facts, decisions, constraints, and prior root causes.
4. Drop transient logs unless they explain the current next action.
5. Do not store or forward secrets, credentials, private reasoning, or scratchpad content.

## Output

FACTS:
- 

DECISIONS:
- 

CONSTRAINTS:
- 

CONFLICTS:
- 

NEXT:
- "#,
    },
    DefaultAgentResourceInput {
        kind: AgentResourceKind::Agent,
        assistant: "generic",
        name: "token_efficient_incident_responder",
        title: "Token Efficient Incident Responder",
        description:
            "Agent profile for concise incident triage, mitigation, and handoff.",
        body: r#"## Role

Act as an incident response agent that minimizes tokens while preserving safety and auditability.

## Operating Rules

1. Retrieve MemoryOps context for prior incidents, runbooks, owners, and rollback constraints.
2. Report current impact before analysis.
3. Preserve exact alerts, services, commands, dashboards, IDs, and timestamps.
4. Prefer mitigation, verification, rollback, and owner handoff over detailed chronology.
5. Store only durable post-incident outcomes after resolution.

## Output

IMPACT:
- 

MITIGATION:
- 

VERIFY:
- 

OWNER:
- 

RISK:
- 

STORE:
- "#,
    },
];

pub async fn seed_skill_resource(
    db: &PgPool,
    workspace_id: Uuid,
    input: SkillResourceInput<'_>,
) -> Result<(), AppError> {
    let metadata = json!({ "seeded": true });
    let resource = sqlx::query_as::<_, AgentResource>(&format!(
        r#"
        INSERT INTO agent_resources (
            workspace_id, kind, assistant, name, filename, title, description,
            body, content, metadata
        )
        VALUES ($1, 'skill', $2, $3, $4, $5, $6, $7, $8, $9)
        ON CONFLICT (workspace_id, kind, assistant, name) DO NOTHING
        RETURNING {AGENT_RESOURCE_COLUMNS}
        "#,
    ))
    .bind(workspace_id)
    .bind(input.assistant)
    .bind(input.name)
    .bind(input.filename)
    .bind(input.title)
    .bind(input.description)
    .bind(input.instructions)
    .bind(input.content)
    .bind(metadata)
    .fetch_optional(db)
    .await
    .map_err(AppError::Database)?;

    if let Some(resource) = resource {
        insert_agent_resource_version_from_pool(
            db,
            &resource,
            Some("seeded initial version"),
            None,
        )
        .await?;
    }

    Ok(())
}

pub async fn create_skill_resource_versioned(
    db: &PgPool,
    workspace_id: Uuid,
    input: SkillResourceInput<'_>,
    change_note: Option<&str>,
    created_by: Option<&str>,
) -> Result<AgentResource, AppError> {
    let metadata = json!({ "source": "legacy-agent-skills-api" });
    let mut tx = db.begin().await.map_err(AppError::Database)?;

    let resource = sqlx::query_as::<_, AgentResource>(&format!(
        r#"
            INSERT INTO agent_resources (
                workspace_id, kind, assistant, name, filename, title, description,
                body, content, metadata
            )
            VALUES ($1, 'skill', $2, $3, $4, $5, $6, $7, $8, $9)
            RETURNING {AGENT_RESOURCE_COLUMNS}
            "#
    ))
    .bind(workspace_id)
    .bind(input.assistant)
    .bind(input.name)
    .bind(input.filename)
    .bind(input.title)
    .bind(input.description)
    .bind(input.instructions)
    .bind(input.content)
    .bind(&metadata)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_agent_resource_write_error)?;

    upsert_legacy_agent_skill(&mut tx, workspace_id, skill_input_from_resource(&resource)).await?;
    insert_agent_resource_version(&mut tx, &resource, change_note, created_by).await?;
    tx.commit().await.map_err(AppError::Database)?;

    Ok(resource)
}

pub async fn upsert_skill_resource_versioned(
    db: &PgPool,
    workspace_id: Uuid,
    input: SkillResourceInput<'_>,
    change_note: Option<&str>,
    created_by: Option<&str>,
) -> Result<AgentResource, AppError> {
    let metadata = json!({ "source": "legacy-agent-skills-api" });
    let mut tx = db.begin().await.map_err(AppError::Database)?;

    let resource = sqlx::query_as::<_, AgentResource>(&format!(
        r#"
            UPDATE agent_resources
            SET filename = $4,
                title = $5,
                description = $6,
                body = $7,
                content = $8,
                metadata = $9,
                version = version + 1
            WHERE workspace_id = $1 AND kind = 'skill' AND assistant = $2 AND name = $3
            RETURNING {AGENT_RESOURCE_COLUMNS}
            "#
    ))
    .bind(workspace_id)
    .bind(input.assistant)
    .bind(input.name)
    .bind(input.filename)
    .bind(input.title)
    .bind(input.description)
    .bind(input.instructions)
    .bind(input.content)
    .bind(&metadata)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    let resource = if let Some(resource) = resource {
        resource
    } else {
        sqlx::query_as::<_, AgentResource>(&format!(
            r#"
                INSERT INTO agent_resources (
                    workspace_id, kind, assistant, name, filename, title, description,
                    body, content, metadata
                )
                VALUES ($1, 'skill', $2, $3, $4, $5, $6, $7, $8, $9)
                RETURNING {AGENT_RESOURCE_COLUMNS}
                "#
        ))
        .bind(workspace_id)
        .bind(input.assistant)
        .bind(input.name)
        .bind(input.filename)
        .bind(input.title)
        .bind(input.description)
        .bind(input.instructions)
        .bind(input.content)
        .bind(&metadata)
        .fetch_one(&mut *tx)
        .await
        .map_err(map_agent_resource_write_error)?
    };

    upsert_legacy_agent_skill(&mut tx, workspace_id, skill_input_from_resource(&resource)).await?;
    insert_agent_resource_version(&mut tx, &resource, change_note, created_by).await?;
    tx.commit().await.map_err(AppError::Database)?;

    Ok(resource)
}

#[axum::debug_handler]
pub async fn list_agent_resources(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Query(query): Query<AgentResourceListQuery>,
) -> AppResult<Json<Vec<AgentResourceSummary>>> {
    let kind = query
        .kind
        .as_deref()
        .map(AgentResourceKind::parse)
        .transpose()?;
    let assistant = query
        .assistant
        .as_deref()
        .map(validate_assistant)
        .transpose()?;

    ensure_default_agent_resources(&state, auth.workspace_id, kind).await?;

    let mut sql = "SELECT id, workspace_id, kind, assistant, name, filename, title, description, \
         metadata, version, created_at, updated_at \
         FROM agent_resources WHERE workspace_id = $1"
        .to_string();
    if kind.is_some() {
        sql.push_str(" AND kind = $2");
    }
    if assistant.is_some() {
        sql.push_str(if kind.is_some() {
            " AND assistant = $3"
        } else {
            " AND assistant = $2"
        });
    }
    sql.push_str(" ORDER BY kind ASC, assistant ASC, LOWER(title) ASC, name ASC");

    let mut query_builder = sqlx::query_as::<_, AgentResourceSummary>(&sql).bind(auth.workspace_id);
    if let Some(kind) = kind {
        query_builder = query_builder.bind(kind.as_str());
    }
    if let Some(assistant) = assistant {
        query_builder = query_builder.bind(assistant);
    }

    let resources = query_builder
        .fetch_all(&state.db)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(resources))
}

#[axum::debug_handler]
pub async fn get_agent_resource(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((kind, assistant, name)): Path<(String, String, String)>,
) -> AppResult<Json<AgentResource>> {
    let kind = AgentResourceKind::parse(&kind)?;
    let assistant = validate_assistant_for_kind(kind, &assistant)?;
    let name = validate_resource_name(&name)?;
    ensure_default_agent_resources(&state, auth.workspace_id, Some(kind)).await?;

    Ok(Json(
        fetch_agent_resource(&state.db, auth.workspace_id, kind, assistant, name).await?,
    ))
}

#[axum::debug_handler]
pub async fn create_agent_resource(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    ctx: Option<Extension<RequestContext>>,
    Json(request): Json<CreateAgentResourceRequest>,
) -> AppResult<Json<AgentResource>> {
    let kind = AgentResourceKind::parse(&request.kind)?;
    let assistant = validate_assistant_for_kind(
        kind,
        request
            .assistant
            .as_deref()
            .unwrap_or(default_assistant(kind)),
    )?;
    let name = validate_resource_name(&request.name)?;
    let title = validate_title(&request.title)?;
    let description = validate_description(&request.description)?;
    let body = validate_body(&request.body)?;
    let content = normalize_content(kind, title, description, &body, request.content.as_deref())?;
    let metadata = validate_metadata(request.metadata.unwrap_or_else(|| json!({})))?;
    let change_note = validate_change_note(request.change_note.as_deref())?;
    let filename = resource_filename(name);
    let actor = auth.actor();

    let mut tx = state.db.begin().await.map_err(AppError::Database)?;

    let resource = sqlx::query_as::<_, AgentResource>(&format!(
        r#"
            INSERT INTO agent_resources (
                workspace_id, kind, assistant, name, filename, title, description,
                body, content, metadata
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
            RETURNING {AGENT_RESOURCE_COLUMNS}
            "#
    ))
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .bind(&filename)
    .bind(title)
    .bind(description)
    .bind(&body)
    .bind(&content)
    .bind(&metadata)
    .fetch_one(&mut *tx)
    .await
    .map_err(map_agent_resource_write_error)?;

    if kind == AgentResourceKind::Skill {
        upsert_legacy_agent_skill(
            &mut tx,
            auth.workspace_id,
            skill_input_from_resource(&resource),
        )
        .await?;
    }
    insert_agent_resource_version(&mut tx, &resource, change_note, Some(actor.as_str())).await?;

    tx.commit().await.map_err(AppError::Database)?;

    let event = AuditEvent::new(
        auth.workspace_id,
        AuditAction::AgentResourceCreated,
        resource.id,
        "agent_resource",
    )
    .actor_api_key(&auth)
    .target_name(resource.name.clone())
    .target_version(resource.version)
    .metadata(json!({
        "kind": resource.kind,
        "assistant": resource.assistant,
        "name": resource.name,
        "version": resource.version,
    }))
    .maybe_request_context(ctx.as_deref());
    write_audit(&state.db, &event)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(resource))
}

#[axum::debug_handler]
pub async fn update_agent_resource(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    ctx: Option<Extension<RequestContext>>,
    Path((kind, assistant, name)): Path<(String, String, String)>,
    Json(request): Json<UpdateAgentResourceRequest>,
) -> AppResult<Json<AgentResource>> {
    let UpdateAgentResourceRequest {
        title,
        description,
        body,
        content,
        metadata,
        change_note,
    } = request;
    let kind = AgentResourceKind::parse(&kind)?;
    let assistant = validate_assistant_for_kind(kind, &assistant)?;
    let name = validate_resource_name(&name)?;
    let change_note = validate_change_note(change_note.as_deref())?;
    let actor = auth.actor();

    let mut tx = state.db.begin().await.map_err(AppError::Database)?;
    let current = sqlx::query_as::<_, ResourceWriteState>(
        r#"
        SELECT title, description, body, content, metadata
        FROM agent_resources
        WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4
        "#,
    )
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("agent_resource:{}/{}/{}", kind.as_str(), assistant, name),
    })?;

    let title_value = match title.as_deref() {
        Some(value) => validate_title(value)?.to_owned(),
        None => current.title,
    };
    let description_value = match description.as_deref() {
        Some(value) => validate_description(value)?.to_owned(),
        None => current.description,
    };
    let body_value = match body.as_deref() {
        Some(value) => validate_body(value)?,
        None => current.body,
    };
    let metadata_value = match metadata {
        Some(value) => validate_metadata(value)?,
        None => current.metadata,
    };
    let content_value =
        if content.is_none() && title.is_none() && description.is_none() && body.is_none() {
            current.content
        } else {
            normalize_content(
                kind,
                &title_value,
                &description_value,
                &body_value,
                content.as_deref(),
            )?
        };

    let resource = sqlx::query_as::<_, AgentResource>(&format!(
        r#"
            UPDATE agent_resources
            SET title = $5,
                description = $6,
                body = $7,
                content = $8,
                metadata = $9,
                version = version + 1
            WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4
            RETURNING {AGENT_RESOURCE_COLUMNS}
            "#
    ))
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .bind(&title_value)
    .bind(&description_value)
    .bind(&body_value)
    .bind(&content_value)
    .bind(&metadata_value)
    .fetch_one(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    if kind == AgentResourceKind::Skill {
        upsert_legacy_agent_skill(
            &mut tx,
            auth.workspace_id,
            skill_input_from_resource(&resource),
        )
        .await?;
    }
    insert_agent_resource_version(&mut tx, &resource, change_note, Some(actor.as_str())).await?;

    tx.commit().await.map_err(AppError::Database)?;

    let event = AuditEvent::new(
        auth.workspace_id,
        AuditAction::AgentResourceUpdated,
        resource.id,
        "agent_resource",
    )
    .actor_api_key(&auth)
    .target_name(resource.name.clone())
    .target_version(resource.version)
    .metadata(json!({
        "kind": resource.kind,
        "assistant": resource.assistant,
        "name": resource.name,
        "version": resource.version,
        "change_note": change_note,
    }))
    .maybe_request_context(ctx.as_deref());
    write_audit(&state.db, &event)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(resource))
}

#[axum::debug_handler]
pub async fn delete_agent_resource(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    ctx: Option<Extension<RequestContext>>,
    Path((kind, assistant, name)): Path<(String, String, String)>,
) -> AppResult<Json<Value>> {
    let kind = AgentResourceKind::parse(&kind)?;
    let assistant = validate_assistant_for_kind(kind, &assistant)?;
    let name = validate_resource_name(&name)?;

    let mut tx = state.db.begin().await.map_err(AppError::Database)?;
    let row: Option<(Uuid,)> = sqlx::query_as(
        r#"
        DELETE FROM agent_resources
        WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4
        RETURNING id
        "#,
    )
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?;

    let Some((resource_id,)) = row else {
        return Err(AppError::NotFound {
            resource: format!("agent_resource:{}/{}/{}", kind.as_str(), assistant, name),
        });
    };

    // Keep the legacy agent_skills table in sync. Without this, deleting a skill
    // here would leave an orphan row that makes the legacy create endpoint report
    // a spurious conflict when the same name is recreated.
    if kind == AgentResourceKind::Skill {
        sqlx::query(
            "DELETE FROM agent_skills WHERE workspace_id = $1 AND assistant = $2 AND name = $3",
        )
        .bind(auth.workspace_id)
        .bind(assistant)
        .bind(name)
        .execute(&mut *tx)
        .await
        .map_err(AppError::Database)?;
    }

    tx.commit().await.map_err(AppError::Database)?;

    let event = AuditEvent::new(
        auth.workspace_id,
        AuditAction::AgentResourceDeleted,
        resource_id,
        "agent_resource",
    )
    .actor_api_key(&auth)
    .target_name(name.to_owned())
    .metadata(json!({
        "kind": kind.as_str(),
        "assistant": assistant,
        "name": name,
    }))
    .maybe_request_context(ctx.as_deref());
    write_audit(&state.db, &event)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(json!({ "deleted": true })))
}

#[axum::debug_handler]
pub async fn list_agent_resource_versions(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((kind, assistant, name)): Path<(String, String, String)>,
) -> AppResult<Json<Vec<AgentResourceVersion>>> {
    let kind = AgentResourceKind::parse(&kind)?;
    let assistant = validate_assistant_for_kind(kind, &assistant)?;
    let name = validate_resource_name(&name)?;

    let versions = sqlx::query_as::<_, AgentResourceVersion>(&format!(
        r#"
            SELECT {AGENT_RESOURCE_VERSION_COLUMNS}
            FROM agent_resource_versions
            WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4
            ORDER BY version DESC
            "#
    ))
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .fetch_all(&state.db)
    .await
    .map_err(AppError::Database)?;

    if versions.is_empty() {
        let _ = fetch_agent_resource(&state.db, auth.workspace_id, kind, assistant, name).await?;
    }

    Ok(Json(versions))
}

#[axum::debug_handler]
pub async fn get_agent_resource_version(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    Path((kind, assistant, name, version)): Path<(String, String, String, i32)>,
) -> AppResult<Json<AgentResourceVersion>> {
    let kind = AgentResourceKind::parse(&kind)?;
    let assistant = validate_assistant_for_kind(kind, &assistant)?;
    let name = validate_resource_name(&name)?;

    let version = sqlx::query_as::<_, AgentResourceVersion>(&format!(
        r#"
            SELECT {AGENT_RESOURCE_VERSION_COLUMNS}
            FROM agent_resource_versions
            WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4 AND version = $5
            "#
    ))
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .bind(version)
    .fetch_optional(&state.db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!(
            "agent_resource_version:{}/{}/{}@{}",
            kind.as_str(),
            assistant,
            name,
            version
        ),
    })?;

    Ok(Json(version))
}

#[axum::debug_handler]
pub async fn rollback_agent_resource(
    State(state): State<AppState>,
    Extension(auth): Extension<AuthContext>,
    ctx: Option<Extension<RequestContext>>,
    Path((kind, assistant, name, version)): Path<(String, String, String, i32)>,
    Json(request): Json<RollbackAgentResourceRequest>,
) -> AppResult<Json<AgentResource>> {
    let kind = AgentResourceKind::parse(&kind)?;
    let assistant = validate_assistant_for_kind(kind, &assistant)?;
    let name = validate_resource_name(&name)?;
    let change_note = validate_change_note(request.change_note.as_deref())?;
    let actor = auth.actor();

    let mut tx = state.db.begin().await.map_err(AppError::Database)?;

    let snapshot = sqlx::query_as::<_, AgentResourceVersion>(&format!(
        r#"
            SELECT {AGENT_RESOURCE_VERSION_COLUMNS}
            FROM agent_resource_versions
            WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4 AND version = $5
            "#
    ))
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .bind(version)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!(
            "agent_resource_version:{}/{}/{}@{}",
            kind.as_str(),
            assistant,
            name,
            version
        ),
    })?;

    let resource = sqlx::query_as::<_, AgentResource>(&format!(
        r#"
            UPDATE agent_resources
            SET filename = $5,
                title = $6,
                description = $7,
                body = $8,
                content = $9,
                metadata = $10,
                version = version + 1
            WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4
            RETURNING {AGENT_RESOURCE_COLUMNS}
            "#
    ))
    .bind(auth.workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .bind(&snapshot.filename)
    .bind(&snapshot.title)
    .bind(&snapshot.description)
    .bind(&snapshot.body)
    .bind(&snapshot.content)
    .bind(&snapshot.metadata)
    .fetch_optional(&mut *tx)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("agent_resource:{}/{}/{}", kind.as_str(), assistant, name),
    })?;

    if kind == AgentResourceKind::Skill {
        upsert_legacy_agent_skill(
            &mut tx,
            auth.workspace_id,
            skill_input_from_resource(&resource),
        )
        .await?;
    }
    let note = change_note
        .map(str::to_owned)
        .unwrap_or_else(|| format!("rollback to v{version}"));
    insert_agent_resource_version(&mut tx, &resource, Some(&note), Some(actor.as_str())).await?;

    tx.commit().await.map_err(AppError::Database)?;

    let event = AuditEvent::new(
        auth.workspace_id,
        AuditAction::AgentResourceRolledBack,
        resource.id,
        "agent_resource",
    )
    .actor_api_key(&auth)
    .target_name(resource.name.clone())
    .target_version(resource.version)
    .metadata(json!({
        "kind": resource.kind,
        "assistant": resource.assistant,
        "name": resource.name,
        "version": resource.version,
        "rolled_back_to": version,
    }))
    .maybe_request_context(ctx.as_deref());
    write_audit(&state.db, &event)
        .await
        .map_err(AppError::Database)?;

    Ok(Json(resource))
}

async fn fetch_agent_resource(
    db: &PgPool,
    workspace_id: Uuid,
    kind: AgentResourceKind,
    assistant: &str,
    name: &str,
) -> AppResult<AgentResource> {
    sqlx::query_as::<_, AgentResource>(&format!(
        "SELECT {AGENT_RESOURCE_COLUMNS} FROM agent_resources \
             WHERE workspace_id = $1 AND kind = $2 AND assistant = $3 AND name = $4"
    ))
    .bind(workspace_id)
    .bind(kind.as_str())
    .bind(assistant)
    .bind(name)
    .fetch_optional(db)
    .await
    .map_err(AppError::Database)?
    .ok_or_else(|| AppError::NotFound {
        resource: format!("agent_resource:{}/{}/{}", kind.as_str(), assistant, name),
    })
}

async fn ensure_default_agent_resources(
    state: &AppState,
    workspace_id: Uuid,
    requested_kind: Option<AgentResourceKind>,
) -> AppResult<()> {
    let kinds = requested_kind
        .map(|kind| vec![kind])
        .unwrap_or_else(|| DEFAULT_AGENT_RESOURCE_KINDS.to_vec());

    for kind in kinds {
        let count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_resources WHERE workspace_id = $1 AND kind = $2",
        )
        .bind(workspace_id)
        .bind(kind.as_str())
        .fetch_one(&state.db)
        .await
        .map_err(AppError::Database)?;

        if count != 0 {
            continue;
        }

        if kind == AgentResourceKind::Skill {
            if let Err(err) =
                super::agent_skills::seed_default_skills(&state.db, workspace_id).await
            {
                tracing::warn!(?err, "failed to auto-seed default agent skills");
            }
            continue;
        }

        for input in DEFAULT_AGENT_RESOURCES
            .iter()
            .copied()
            .filter(|resource| resource.kind == kind)
        {
            if let Err(err) = seed_default_agent_resource(&state.db, workspace_id, input).await {
                tracing::warn!(
                    ?err,
                    kind = kind.as_str(),
                    name = input.name,
                    "failed to auto-seed default agent resource"
                );
            }
        }
    }

    Ok(())
}

pub async fn seed_all_default_agent_resources(
    state: &AppState,
    workspace_id: Uuid,
) -> AppResult<()> {
    ensure_default_agent_resources(state, workspace_id, None).await
}

async fn seed_default_agent_resource(
    db: &PgPool,
    workspace_id: Uuid,
    input: DefaultAgentResourceInput,
) -> Result<(), AppError> {
    let filename = resource_filename(input.name);
    let content = compose_resource_markdown(input.kind, input.title, input.description, input.body);
    let metadata = json!({
        "seeded": true,
        "default": true,
        "source": "memoryops-defaults",
    });

    let resource = sqlx::query_as::<_, AgentResource>(&format!(
        r#"
        INSERT INTO agent_resources (
            workspace_id, kind, assistant, name, filename, title, description,
            body, content, metadata
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (workspace_id, kind, assistant, name) DO NOTHING
        RETURNING {AGENT_RESOURCE_COLUMNS}
        "#,
    ))
    .bind(workspace_id)
    .bind(input.kind.as_str())
    .bind(input.assistant)
    .bind(input.name)
    .bind(&filename)
    .bind(input.title)
    .bind(input.description)
    .bind(input.body)
    .bind(&content)
    .bind(&metadata)
    .fetch_optional(db)
    .await
    .map_err(AppError::Database)?;

    if let Some(resource) = resource {
        insert_agent_resource_version_from_pool(
            db,
            &resource,
            Some("seeded default resource"),
            None,
        )
        .await?;
    }

    Ok(())
}

async fn insert_agent_resource_version(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    resource: &AgentResource,
    change_note: Option<&str>,
    created_by: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO agent_resource_versions (
            resource_id, workspace_id, kind, assistant, name, filename, title,
            description, body, content, metadata, version, change_note, created_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT (resource_id, version) DO NOTHING
        "#,
    )
    .bind(resource.id)
    .bind(resource.workspace_id)
    .bind(&resource.kind)
    .bind(&resource.assistant)
    .bind(&resource.name)
    .bind(&resource.filename)
    .bind(&resource.title)
    .bind(&resource.description)
    .bind(&resource.body)
    .bind(&resource.content)
    .bind(&resource.metadata)
    .bind(resource.version)
    .bind(change_note)
    .bind(created_by)
    .execute(&mut **tx)
    .await
    .map_err(AppError::Database)?;

    Ok(())
}

async fn insert_agent_resource_version_from_pool(
    db: &PgPool,
    resource: &AgentResource,
    change_note: Option<&str>,
    created_by: Option<&str>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO agent_resource_versions (
            resource_id, workspace_id, kind, assistant, name, filename, title,
            description, body, content, metadata, version, change_note, created_by
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
        ON CONFLICT (resource_id, version) DO NOTHING
        "#,
    )
    .bind(resource.id)
    .bind(resource.workspace_id)
    .bind(&resource.kind)
    .bind(&resource.assistant)
    .bind(&resource.name)
    .bind(&resource.filename)
    .bind(&resource.title)
    .bind(&resource.description)
    .bind(&resource.body)
    .bind(&resource.content)
    .bind(&resource.metadata)
    .bind(resource.version)
    .bind(change_note)
    .bind(created_by)
    .execute(db)
    .await
    .map_err(AppError::Database)?;

    Ok(())
}

async fn upsert_legacy_agent_skill(
    tx: &mut sqlx::Transaction<'_, Postgres>,
    workspace_id: Uuid,
    input: SkillResourceInput<'_>,
) -> AppResult<()> {
    sqlx::query(
        r#"
        INSERT INTO agent_skills (
            workspace_id, name, filename, assistant, title, description, instructions, content
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (workspace_id, assistant, name) DO UPDATE
        SET filename = EXCLUDED.filename,
            title = EXCLUDED.title,
            description = EXCLUDED.description,
            instructions = EXCLUDED.instructions,
            content = EXCLUDED.content,
            updated_at = NOW()
        "#,
    )
    .bind(workspace_id)
    .bind(input.name)
    .bind(input.filename)
    .bind(input.assistant)
    .bind(input.title)
    .bind(input.description)
    .bind(input.instructions)
    .bind(input.content)
    .execute(&mut **tx)
    .await
    .map_err(AppError::Database)?;

    Ok(())
}

fn skill_input_from_resource(resource: &AgentResource) -> SkillResourceInput<'_> {
    SkillResourceInput {
        assistant: &resource.assistant,
        name: &resource.name,
        filename: &resource.filename,
        title: &resource.title,
        description: &resource.description,
        instructions: &resource.body,
        content: &resource.content,
    }
}

fn validate_assistant(value: &str) -> AppResult<&str> {
    let trimmed = value.trim();
    match trimmed {
        "generic" | "openai" | "claude" | "gemini" => Ok(trimmed),
        _ => Err(AppError::Validation(
            "Assistant must be one of generic, openai, claude, or gemini".to_owned(),
        )),
    }
}

fn validate_assistant_for_kind(kind: AgentResourceKind, value: &str) -> AppResult<&str> {
    let assistant = validate_assistant(value)?;
    if kind == AgentResourceKind::Skill && assistant != "claude" && assistant != "gemini" {
        return Err(AppError::Validation(
            "Skill resources must target either claude or gemini".to_owned(),
        ));
    }
    Ok(assistant)
}

fn default_assistant(kind: AgentResourceKind) -> &'static str {
    match kind {
        AgentResourceKind::Skill => "claude",
        AgentResourceKind::Agent | AgentResourceKind::Prompt | AgentResourceKind::Instruction => {
            "generic"
        }
    }
}

fn validate_resource_name(name: &str) -> AppResult<&str> {
    let trimmed = name.trim();
    let mut chars = trimmed.chars();
    let Some(first) = chars.next() else {
        return Err(AppError::Validation("Resource name is required".to_owned()));
    };

    if trimmed.len() > MAX_RESOURCE_NAME_LEN {
        return Err(AppError::Validation(format!(
            "Resource name must be at most {MAX_RESOURCE_NAME_LEN} characters"
        )));
    }
    if !first.is_ascii_lowercase() {
        return Err(AppError::Validation(
            "Resource name must start with a lowercase letter".to_owned(),
        ));
    }
    if !trimmed
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_' || c == '-')
    {
        return Err(AppError::Validation(
            "Resource name may only contain lowercase letters, digits, underscores, and hyphens"
                .to_owned(),
        ));
    }

    Ok(trimmed)
}

fn validate_title(title: &str) -> AppResult<&str> {
    validate_single_line_text(title, "Resource title", MAX_RESOURCE_TITLE_LEN)
}

fn validate_description(description: &str) -> AppResult<&str> {
    validate_single_line_text(
        description,
        "Resource description",
        MAX_RESOURCE_DESCRIPTION_LEN,
    )
}

fn validate_single_line_text<'a>(
    value: &'a str,
    label: &str,
    max_len: usize,
) -> AppResult<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(format!("{label} is required")));
    }
    if trimmed.len() > max_len {
        return Err(AppError::Validation(format!(
            "{label} must be at most {max_len} characters"
        )));
    }
    if trimmed.contains('\n') || trimmed.contains('\r') {
        return Err(AppError::Validation(format!(
            "{label} must be a single line"
        )));
    }
    Ok(trimmed)
}

fn validate_body(body: &str) -> AppResult<String> {
    let normalized = normalize_newlines(body);
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("Resource body is required".to_owned()));
    }
    if trimmed.len() > MAX_RESOURCE_BODY_LEN {
        return Err(AppError::Validation(format!(
            "Resource body must be at most {MAX_RESOURCE_BODY_LEN} characters"
        )));
    }
    Ok(trimmed.to_owned())
}

fn validate_content(content: &str) -> AppResult<String> {
    let normalized = normalize_newlines(content);
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(
            "Resource content is required".to_owned(),
        ));
    }
    if trimmed.len() > MAX_RESOURCE_CONTENT_LEN {
        return Err(AppError::Validation(format!(
            "Resource content must be at most {MAX_RESOURCE_CONTENT_LEN} characters"
        )));
    }
    Ok(trimmed.to_owned())
}

fn validate_metadata(metadata: Value) -> AppResult<Value> {
    if metadata.is_object() {
        Ok(metadata)
    } else {
        Err(AppError::Validation(
            "Resource metadata must be a JSON object".to_owned(),
        ))
    }
}

fn validate_change_note(value: Option<&str>) -> AppResult<Option<&str>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > MAX_CHANGE_NOTE_LEN {
        return Err(AppError::Validation(format!(
            "Change note must be at most {MAX_CHANGE_NOTE_LEN} characters"
        )));
    }
    Ok(Some(trimmed))
}

fn normalize_content(
    kind: AgentResourceKind,
    title: &str,
    description: &str,
    body: &str,
    content: Option<&str>,
) -> AppResult<String> {
    if let Some(content) = content {
        return validate_content(content);
    }
    Ok(compose_resource_markdown(kind, title, description, body))
}

fn compose_resource_markdown(
    kind: AgentResourceKind,
    title: &str,
    description: &str,
    body: &str,
) -> String {
    let trimmed_body = body.trim();
    if trimmed_body.is_empty() {
        format!(
            "# {}: {title}\n\n**Description:** {description}\n",
            kind.title_label()
        )
    } else {
        format!(
            "# {}: {title}\n\n**Description:** {description}\n\n{trimmed_body}\n",
            kind.title_label()
        )
    }
}

fn resource_filename(name: &str) -> String {
    format!("{name}.md")
}

fn normalize_newlines(value: &str) -> String {
    value.replace("\r\n", "\n").replace('\r', "\n")
}

fn map_agent_resource_write_error(error: sqlx::Error) -> AppError {
    if let sqlx::Error::Database(db_error) = &error {
        if db_error.code().as_deref() == Some("23505") {
            return AppError::Conflict(
                "An agent resource with this kind, assistant, and name already exists".to_owned(),
            );
        }
    }
    AppError::Database(error)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]

    use super::*;
    use axum::{
        extract::{Path, Query, State},
        Extension, Json,
    };
    use common::providers::{FastEmbedProvider, OllamaProvider};
    use common::{AppConfig, AppState};
    use qdrant_client::Qdrant;
    use serde_json::json;
    use sqlx::PgPool;
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    #[test]
    fn skill_resources_are_limited_to_claude_and_gemini() {
        assert!(validate_assistant_for_kind(AgentResourceKind::Skill, "claude").is_ok());
        assert!(validate_assistant_for_kind(AgentResourceKind::Skill, "gemini").is_ok());
        assert!(validate_assistant_for_kind(AgentResourceKind::Skill, "generic").is_err());
    }

    #[test]
    fn non_skill_resources_accept_all_assistants() {
        for kind in [
            AgentResourceKind::Agent,
            AgentResourceKind::Prompt,
            AgentResourceKind::Instruction,
        ] {
            for assistant in ["generic", "openai", "claude", "gemini"] {
                assert!(validate_assistant_for_kind(kind, assistant).is_ok());
            }
        }
    }

    #[test]
    fn validation_rejects_invalid_names_and_metadata() {
        assert!(validate_resource_name("BadName").is_err());
        assert!(validate_resource_name("bad/name").is_err());
        assert!(validate_metadata(json!(["not", "object"])).is_err());
        assert!(validate_metadata(json!({ "source": "test" })).is_ok());
    }

    #[test]
    fn default_agent_resources_are_valid_and_unique() {
        let mut keys = std::collections::HashSet::new();

        for input in DEFAULT_AGENT_RESOURCES {
            let key = format!("{}:{}:{}", input.kind.as_str(), input.assistant, input.name);
            assert!(keys.insert(key), "duplicate default resource");
            assert!(validate_assistant_for_kind(input.kind, input.assistant).is_ok());
            assert!(validate_resource_name(input.name).is_ok());
            assert!(validate_title(input.title).is_ok());
            assert!(validate_description(input.description).is_ok());
            assert!(validate_body(input.body).is_ok());
        }
    }

    #[test]
    fn body_and_content_are_trimmed_and_newline_normalized() {
        let body = validate_body("\r\n## Trigger\r\n- Run\r\n").expect("valid body");
        assert_eq!(body, "## Trigger\n- Run");

        let content = validate_content("\r# Prompt: Test\rBody\r\n").expect("valid content");
        assert_eq!(content, "# Prompt: Test\nBody");
    }

    #[test]
    fn compose_resource_markdown_uses_kind_label() {
        let content = compose_resource_markdown(
            AgentResourceKind::Prompt,
            "Release Brief",
            "Drafts concise release notes",
            "Summarize changes in three bullets.",
        );

        assert!(content.starts_with("# Prompt: Release Brief"));
        assert!(content.contains("**Description:** Drafts concise release notes"));
        assert!(content.ends_with("Summarize changes in three bullets.\n"));
    }

    #[sqlx::test(migrations = "../../migrations")]
    async fn agent_resource_lifecycle_covers_kinds_versions_and_legacy_sync(pool: PgPool) {
        let workspace_id = Uuid::now_v7();
        insert_workspace(&pool, workspace_id).await;
        let state = test_state(pool.clone());
        let auth = test_auth(workspace_id);

        let skill = create_agent_resource(
            State(state.clone()),
            Extension(auth.clone()),
            None,
            Json(CreateAgentResourceRequest {
                kind: "skill".to_owned(),
                assistant: Some("claude".to_owned()),
                name: "memoryops_usage".to_owned(),
                title: "MemoryOps Usage".to_owned(),
                description: "Guides Claude to use MemoryOps context.".to_owned(),
                body: "## Trigger\r\n- Before project work\r\n".to_owned(),
                content: None,
                metadata: Some(json!({ "seeded": false })),
                change_note: Some("Initial skill".to_owned()),
            }),
        )
        .await
        .expect("skill create should succeed")
        .0;

        assert_eq!(skill.kind, "skill");
        assert_eq!(skill.assistant, "claude");
        assert_eq!(skill.version, 1);
        assert_eq!(skill.body, "## Trigger\n- Before project work");
        assert!(skill.content.starts_with("# Skill: MemoryOps Usage"));

        let legacy_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_skills WHERE workspace_id = $1 AND assistant = 'claude' AND name = 'memoryops_usage'",
        )
        .bind(workspace_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(legacy_count, 1);

        let mut prompt_generic_content = String::new();
        for kind in ["agent", "prompt", "instruction"] {
            for assistant in ["generic", "openai", "claude", "gemini"] {
                let name = format!("{kind}_{assistant}");
                let created = create_agent_resource(
                    State(state.clone()),
                    Extension(auth.clone()),
                    None,
                    Json(CreateAgentResourceRequest {
                        kind: kind.to_owned(),
                        assistant: Some(assistant.to_owned()),
                        name: name.clone(),
                        title: format!("{assistant} {kind}"),
                        description: format!("Test {kind} for {assistant}."),
                        body: "## Body\nReusable guidance.".to_owned(),
                        content: None,
                        metadata: Some(json!({ "assistant": assistant })),
                        change_note: None,
                    }),
                )
                .await
                .expect("resource create should succeed")
                .0;

                assert_eq!(created.kind, kind);
                assert_eq!(created.assistant, assistant);
                assert_eq!(created.version, 1);

                if kind == "prompt" && assistant == "generic" {
                    prompt_generic_content = created.content;
                }
            }
        }

        let duplicate = create_agent_resource(
            State(state.clone()),
            Extension(auth.clone()),
            None,
            Json(CreateAgentResourceRequest {
                kind: "skill".to_owned(),
                assistant: Some("claude".to_owned()),
                name: "memoryops_usage".to_owned(),
                title: "MemoryOps Usage".to_owned(),
                description: "Guides Claude to use MemoryOps context.".to_owned(),
                body: "## Trigger\n- Duplicate".to_owned(),
                content: None,
                metadata: None,
                change_note: None,
            }),
        )
        .await
        .expect_err("duplicate resource should conflict");
        assert!(matches!(duplicate, AppError::Conflict(_)));

        let metadata_only = update_agent_resource(
            State(state.clone()),
            Extension(auth.clone()),
            None,
            Path((
                "prompt".to_owned(),
                "generic".to_owned(),
                "prompt_generic".to_owned(),
            )),
            Json(UpdateAgentResourceRequest {
                title: None,
                description: None,
                body: None,
                content: None,
                metadata: Some(json!({ "default": false, "labels": ["review"] })),
                change_note: Some("metadata only".to_owned()),
            }),
        )
        .await
        .expect("metadata update should succeed")
        .0;
        assert_eq!(metadata_only.version, 2);
        assert_eq!(metadata_only.content, prompt_generic_content);
        assert_eq!(metadata_only.metadata["labels"][0], "review");

        let content_only = update_agent_resource(
            State(state.clone()),
            Extension(auth.clone()),
            None,
            Path((
                "prompt".to_owned(),
                "generic".to_owned(),
                "prompt_generic".to_owned(),
            )),
            Json(UpdateAgentResourceRequest {
                title: None,
                description: None,
                body: None,
                content: Some("# Prompt: Override\r\n\r\nCustom export body.\r\n".to_owned()),
                metadata: None,
                change_note: Some("custom content".to_owned()),
            }),
        )
        .await
        .expect("content update should succeed")
        .0;
        assert_eq!(content_only.version, 3);
        assert_eq!(content_only.body, "## Body\nReusable guidance.");
        assert_eq!(
            content_only.content,
            "# Prompt: Override\n\nCustom export body."
        );

        let rolled_back = rollback_agent_resource(
            State(state.clone()),
            Extension(auth.clone()),
            None,
            Path((
                "prompt".to_owned(),
                "generic".to_owned(),
                "prompt_generic".to_owned(),
                1,
            )),
            Json(RollbackAgentResourceRequest {
                change_note: Some("restore initial prompt".to_owned()),
            }),
        )
        .await
        .expect("rollback should succeed")
        .0;
        assert_eq!(rolled_back.version, 4);
        assert_eq!(rolled_back.content, prompt_generic_content);

        let missing_version = get_agent_resource_version(
            State(state.clone()),
            Extension(auth.clone()),
            Path((
                "prompt".to_owned(),
                "generic".to_owned(),
                "prompt_generic".to_owned(),
                99,
            )),
        )
        .await
        .expect_err("missing version should 404");
        assert!(matches!(missing_version, AppError::NotFound { .. }));

        let versions = list_agent_resource_versions(
            State(state.clone()),
            Extension(auth.clone()),
            Path((
                "prompt".to_owned(),
                "generic".to_owned(),
                "prompt_generic".to_owned(),
            )),
        )
        .await
        .expect("versions should list")
        .0;
        assert_eq!(versions.len(), 4);
        assert_eq!(versions[0].version, 4);

        let _ = delete_agent_resource(
            State(state.clone()),
            Extension(auth.clone()),
            None,
            Path((
                "skill".to_owned(),
                "claude".to_owned(),
                "memoryops_usage".to_owned(),
            )),
        )
        .await
        .expect("delete should succeed");
        let legacy_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM agent_skills WHERE workspace_id = $1 AND assistant = 'claude' AND name = 'memoryops_usage'",
        )
        .bind(workspace_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(legacy_count, 0);

        let other_workspace_id = Uuid::now_v7();
        insert_workspace(&pool, other_workspace_id).await;
        let other_auth = test_auth(other_workspace_id);
        let _ = create_agent_resource(
            State(state.clone()),
            Extension(other_auth.clone()),
            None,
            Json(CreateAgentResourceRequest {
                kind: "prompt".to_owned(),
                assistant: Some("generic".to_owned()),
                name: "prompt_generic".to_owned(),
                title: "Other Workspace Prompt".to_owned(),
                description: "Same name in a different workspace.".to_owned(),
                body: "## Body\nWorkspace isolated.".to_owned(),
                content: None,
                metadata: None,
                change_note: None,
            }),
        )
        .await
        .expect("same resource name should be allowed in another workspace");

        let listed = list_agent_resources(
            State(state),
            Extension(auth),
            Query(AgentResourceListQuery {
                kind: Some("prompt".to_owned()),
                assistant: Some("generic".to_owned()),
            }),
        )
        .await
        .expect("list should succeed")
        .0;
        assert_eq!(
            listed
                .iter()
                .filter(|resource| resource.name == "prompt_generic")
                .count(),
            1
        );
        assert!(listed
            .iter()
            .all(|resource| resource.workspace_id == workspace_id));
    }

    async fn insert_workspace(pool: &PgPool, workspace_id: Uuid) {
        // `workspaces.name` is UNIQUE, and this test seeds two workspaces to
        // exercise cross-workspace isolation, so derive a unique name per id.
        sqlx::query("INSERT INTO workspaces (id, name) VALUES ($1, $2)")
            .bind(workspace_id)
            .bind(format!("test-ws-{workspace_id}"))
            .execute(pool)
            .await
            .unwrap();
    }

    fn test_auth(workspace_id: Uuid) -> AuthContext {
        AuthContext {
            workspace_id,
            key_id: Uuid::now_v7(),
            key_prefix: "prefix".to_owned(),
        }
    }

    fn test_state(pool: PgPool) -> AppState {
        AppState {
            db: pool,
            redis: deadpool_redis::Config::from_url("redis://localhost:16379")
                .create_pool(None)
                .unwrap(),
            qdrant: Qdrant::from_url("http://localhost:16333").build().unwrap(),
            processor_semaphore: Arc::new(Semaphore::new(1)),
            embedding_provider: Arc::new(FastEmbedProvider::new("test")),
            llm_provider: Arc::new(OllamaProvider::new("http://localhost:9", "test", 1, None)),
            config: Arc::new(
                AppConfig::from_toml_str(include_str!("../../../../config.toml")).unwrap(),
            ),
            app_secret_key: Arc::new(zeroize::Zeroizing::new("secret".to_owned())),
            trusted_proxy_cidrs: Arc::new(Vec::new()),
        }
    }
}
