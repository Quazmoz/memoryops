# Skill: Use MemoryOps Context Registry

**Description:** Interfaces with the running local MemoryOps database and vector search server to retrieve episodic/semantic memories, log new engineering decisions, ingest observations, or discover workspace skills.

## Trigger
Use this skill when:
- The user asks to "retrieve context", "search memories", "query memoryops", or "recall details" about past events or decisions.
- Starting a new task and needing to gather historical context (e.g. "find any existing architectural decisions or guidelines").
- Completing a task/refactoring and needing to save the decision for future agent runs or humans.
- Submitting raw logs or developer observations for background classification.

## Execution Steps

1. **Locate Credentials**
   - Search the workspace directory (and parent directories) for the `.memoryops.local.json` file.
   - Read this file to extract `api_key` and `workspace_id`.
   - If missing, check for environment variables `MEMORYOPS_API_KEY` and `MEMORYOPS_WORKSPACE_ID`.
   - If no credentials can be found, ask the user to initialize the MemoryOps stack first or provide the key.

2. **Retrieve Context (Task Startup)**
   - Before executing code modifications, check MemoryOps for existing context:
     ```bash
     node scripts/memoryops-client.js retrieve "<search query>"
     ```
   - Review the returned memories to identify guidelines, standards, or past decisions.

3. **Store Decisions (Task Completion)**
   - Once a task or refactor is completed, store it:
     ```bash
     node scripts/memoryops-client.js store "<description of the change/decision>" <tag1> <tag2> ...
     ```
   - Keep description clear and concise. E.g.: `node scripts/memoryops-client.js store "Switched REDIS_URL to use docker-compose DNS name 'redis' instead of localhost" config docker`

4. **Observe Raw Events**
   - Submit unstructured notes or errors to the background processor:
     ```bash
     node scripts/memoryops-client.js observe "<raw observation content>" <tag1> ...
     ```

5. **Interact via MCP (If Configured)**
   - If the repository has a `.mcp.json` or you have the `memoryops` MCP server configured, you can call the following native tools directly instead of spawning the Node CLI script:
     - `memory_retrieve` (query context)
     - `memory_store` (store a memory)
     - `memory_observe` (send an observation)
     - `memory_search` (filtered memory search)
