# Skill: Manage Memory Contradictions

**Description:** Reviews conflicting MemoryOps facts and resolves or escalates them without amplifying stale, unsafe, or out-of-scope context.

## Trigger

Use this skill when:
- MemoryOps returns conflicting memories for a task.
- A user says a stored memory is wrong, stale, or incomplete.
- Promotion, review, or retrieval surfaces contradiction flags.
- New evidence invalidates an older decision, configuration, or incident summary.

## Execution Steps

1. **Collect the Claims**
   - Retrieve the conflicting memories or contradiction flag details.
   - Capture each claim, timestamp, source, scope, confidence, and related repository or service.

2. **Compare Scope and Evidence**
   - Check whether both claims can be true in different workspaces, repos, branches, services, users, or time windows.
   - Prefer newer information only when it is in the same scope and directly supersedes the older claim.
   - Treat logs, migrations, release notes, and explicit user corrections as stronger evidence than loose summaries.

3. **Choose a Resolution**
   - Keep both when the claims are scoped differently and both remain useful.
   - Keep one when evidence clearly shows the other is stale or incorrect.
   - Dismiss only when the contradiction is a false positive and no memory should change.
   - Ask the user before destructive cleanup when evidence is incomplete.

4. **Record the Outcome**
   - Resolve the flag through MemoryOps tools or API when available.
   - Store a concise follow-up memory only if the resolution creates a durable rule or updated source of truth.

## Failure Handling

- If contradiction tools are unavailable, summarize the claims and avoid relying on either as definitive.
- If the conflict involves secrets, credentials, or personal data, do not quote sensitive values; describe the issue generically.

## Output Expectations

- State which claim is safe to use, why, and what action was taken.
- Include unresolved questions when operator confirmation is still needed.
