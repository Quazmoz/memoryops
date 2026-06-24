# Skill: Token Slim

**Description:** Guides coding and DevOps agents to reduce response tokens while preserving technical accuracy, safety, and user intent.

## Trigger

Use this skill when the user asks for:
- reduce tokens
- compact mode
- less verbose
- token slim
- terse but complete
- brief but accurate

## Rules

- Remove filler, greetings, repetitive caveats, and obvious explanations.
- Preserve commands, code, file paths, API fields, config keys, version numbers, error messages, and warnings exactly.
- Never omit safety, security, migration, rollback, data-loss, or compatibility caveats.
- Use terse bullets and exact next actions.
- Prefer changed files, root cause, patch target, tests run, and residual risk over narrative summary.
- Do not store secrets, private reasoning, or transient scratchpad content in MemoryOps.

## Compression Levels

- Light: remove avoidable verbosity with no intentional performance loss.
- Medium: reduce explanation depth while preserving code quality, safety, and verification.
- Heavy: use minimum useful tokens; allow only small tradeoffs in convenience or optional context.
- Ultra: use only when explicitly requested; preserve blockers, warnings, assumptions, and exact technical values.

## Related Agent Library Defaults

- Use `token_efficiency_routing` to choose the restriction level.
- Use `token_budget_policy`, `exactness_preservation`, and `tool_output_compression` as supporting instructions.
- Use `compact_patch_plan`, `compact_review_findings`, `compact_debug_report`, or `compact_final_handoff` for task-specific prompts.
- Use `token_restriction_light`, `token_restriction_medium`, or `token_restriction_heavy` when a whole agent profile should run under that budget.

## Output Expectations

- Lead with the answer or action taken.
- Include only context that changes the next action.
- Label unknown, untested, not verified, and assumption statements when they matter.
