# Skill: Release Notes

**Description:** Retrieves and summarizes code changes, commits, and pull requests to compile formatted release notes.

## Trigger
Use this skill when:
- Preparing a deployment, a new release tag, or a production migration.
- The user requests a summary of changes since a specific version or date.

## Execution Steps
1. **Gather Context**
   - Query MemoryOps using `retrieve` for deployment events, PR merges, and commits within the relevant timeframe.
   - Example query: "PRs merged in last 7 days", "v0.15.0 deployment details".
2. **Draft Release Notes**
   - Categorize the retrieved episodic memories into:
     - 🚀 Features (New capabilities, user-facing enhancements)
     - 🐛 Bug Fixes (Patches, stability fixes, error handling)
     - ⚙️ Maintenance & Infra (CI/CD, dependencies, config updates)
   - For each item, provide a brief description, contributor (if available), and reference the issue/PR number.
3. **Log the Release**
   - Once finalized, store the release summary in the permanent semantic memory registry:
     - Example: `node scripts/memoryops-client.js store "Released v0.16.0 containing hybrid search ranking fixes, HNSW parameter tuning, and 3 bug fixes." releases v0.16.0`
