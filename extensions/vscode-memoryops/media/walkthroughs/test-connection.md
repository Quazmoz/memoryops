## Verify the connection

Run **`MemoryOps: Test Connection`** (the button below) to confirm the extension
can reach your backend and authenticate.

You should see **"MemoryOps connection is healthy."** and the status bar item
will switch to **$(check) MemoryOps**.

If it fails:

- Double-check `memoryops.apiUrl` is reachable from this machine.
- Confirm your API key matches the workspace in `memoryops.workspaceId`.
- Open the **MemoryOps** output channel (`View → Output → MemoryOps`) for the
  full request log.

> 🔄 Transient backend hiccups are retried automatically with exponential
> backoff (tune via `memoryops.maxRetries` / `memoryops.retryBackoffMs`).
