# Skill: Incident Brief

**Description:** Retrieves post-mortem records and past incident triages to assist in diagnosing and documenting active production incidents.

## Trigger
Use this skill when:
- An active production alert fires or a Sev-1 incident is declared.
- The user asks for help troubleshooting a service failure, spike in latency, or connection timeout.
- Compiling a post-mortem or incident summary after resolution.

## Execution Steps
1. **Analyze Incident Symptoms**
   - Note the affected services, error messages, and timestamps.
2. **Query Past Incident History**
   - Search the MemoryOps database for similar past incidents, runbooks, or post-mortem entries.
   - Example query: "Qdrant connection refused", "postgres memory limit spike", "redis stream backpressure".
3. **Formulate Triage Recommendations**
   - Compare the active incident's symptoms with past incidents.
   - Present the user with the most likely root causes and verified recovery steps from past runbooks.
4. **Log Incident & Resolution**
   - After the incident is mitigated, document the event, root cause, and remediation steps in MemoryOps:
     - Example: `node scripts/memoryops-client.js store "Mitigated Redis Streams backpressure by increasing partition count and restarting consumer group. Root cause: slack event spike." incident telemetry`
