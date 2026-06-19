# Skill: Use MemoryOps Context Registry

**Description:** Retrieves durable workspace context from MemoryOps and stores only decisions or observations that will help future agent runs.

## Trigger

Use this skill when:
- Starting a coding, debugging, migration, release, or incident task that may depend on prior project decisions.
- The user asks to retrieve, search, remember, store, or observe MemoryOps context.
- Completing work that produced a stable decision, root cause, policy, migration note, or reusable operational lesson.

## Execution Steps

1. **Locate Credentials**
   - Prefer an existing MemoryOps MCP connection if one is configured.
   - Otherwise look for `MEMORYOPS_API_KEY`, `MEMORYOPS_WORKSPACE_ID`, and `MEMORYOPS_API_URL`.
   - If a local helper exists, use `node scripts/memoryops-client.js`; do not print or store plaintext API keys.

2. **Retrieve Before Acting**
   - Query for concrete identifiers from the task: service names, files, dependencies, incidents, or config keys.
   - Use at least one broader follow-up query for related architecture decisions or known pitfalls.
   - Prefer recent, scoped, and corroborated memories. Treat stale or conflicting memories as evidence, not automatic truth.

3. **Use Context Carefully**
   - Apply retrieved memories only when they match the current workspace, repository, service, and time horizon.
   - If memory conflicts with code or user instructions, pause and explain the conflict before proceeding.

4. **Store Durable Outcomes**
   - Store concise memories for decisions, resolved causes, stable preferences, migration results, and reusable workflow rules.
   - Use observation ingestion for raw logs, symptoms, partial hypotheses, or notes that need later classification.
   - Skip transient task steps, private reasoning, secrets, credentials, and noisy intermediate output.

## Failure Handling

- If credentials or MCP tools are unavailable, continue with local repo inspection and tell the user MemoryOps context was unavailable.
- If retrieval returns nothing useful, say so briefly and avoid inventing historical context.
- If storing fails, preserve the candidate memory in the final handoff so the user can retry.

## Output Expectations

- Mention MemoryOps findings only when they materially changed the work.
- When storing, use a short factual sentence plus tags such as subsystem, tool, incident, dependency, or policy.
