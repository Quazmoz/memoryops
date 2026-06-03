# Skill: Use MemoryOps Context Registry

**Description:** Interfaces with the running local MemoryOps database and vector search server to retrieve episodic/semantic memories, log new engineering decisions, ingest observations, or discover workspace skills.

## Trigger
Use this skill when:
- Resolving queries about recent system architecture decisions, past deployment logs, incident triages, or developer workflows.
- Starting a new task and needing to fetch relevant context from memory.
- Completing a task or refactor and needing to store the decision in the permanent semantic memory registry.
- Logging raw developer observations for async categorization.

## Execution Steps

1. **Locate Credentials**
   - Check the directory hierarchy for `.memoryops.local.json`.
   - Read the file to extract the `api_key` and `workspace_id`.
   - If not found, check the environment for `MEMORYOPS_API_KEY` and `MEMORYOPS_WORKSPACE_ID`. If these are absent, ask the user to provide them or to ensure MemoryOps has been initialized.

2. **Retrieve Context (Task Startup)**
   - Before executing code changes, query existing memories to find out if there are related guidelines, rules, or past issues.
   - Run the client command:
     ```bash
     node scripts/memoryops-client.js retrieve "<search query>"
     ```
   - *Example queries:* "Qdrant ports", "deployment guidelines", "incident alerts".

3. **Store Key Decisions (Task Completion)**
   - When finishing a significant refactor or config change, store it to ensure future agent runs or teammates can query it.
   - Run:
     ```bash
     node scripts/memoryops-client.js store "<brief description of the decision>" [optional-tags]
     ```
   - *Example:* `node scripts/memoryops-client.js store "Configured HNSW indexing tuning on Qdrant collections to improve recall from 0.82 to 0.89" infra qdrant`

4. **Observe Raw Events**
   - If there are unstructured notes, error messages, or raw traces you want analyzed asynchronously later, run:
     ```bash
     node scripts/memoryops-client.js observe "<raw observation content>" [optional-tags]
     ```

5. **Interact via MCP (Alternative)**
   - If the project has a `.mcp.json` or you have a connection to the MemoryOps MCP server, you can call the native `memory_retrieve`, `memory_store`, or `memory_observe` tools directly instead of executing the CLI script.
