# Skill: Manage Memory Contradictions

**Description:** Detects, analyzes, and resolves conflicting information in the MemoryOps context database.

## Trigger
Use this skill when:
- The system flags new memory contradictions during episodic-to-semantic promotion.
- Querying MemoryOps returns conflicting statements (e.g., conflicting dependency versions, config settings, or incident timelines).
- The user requests a review of current memory conflicts or discrepancies.

## Execution Steps
1. **List Contradictions**
   - Query the contradiction list using MCP or curl:
     - curl: `curl -H "X-API-Key: <api-key>" "http://localhost:8080/v1/contradictions?workspace_id=<workspace-id>"`
     - MCP: Call the `memory_list_contradictions` tool.
2. **Analyze the Conflicts**
   - Inspect the conflicting memory units A and B returned by the endpoint. Compare their timestamps (`occurred_at`), author/agents, and context.
3. **Resolve the Contradiction**
   - Select the correct/most up-to-date memory and resolve the contradiction:
     - curl: `POST /v1/contradictions/{id}/resolve` with resolution: `keep_a`, `keep_b`, or `discard_both`.
     - MCP: Call the `memory_resolve_contradiction` tool.
   - Example decision: "Keep the newer configuration memory (A) from v0.16.0 deployment and discard the stale v0.15.0 memory (B)."
