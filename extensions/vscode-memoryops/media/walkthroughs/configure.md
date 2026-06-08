## Connect to your MemoryOps backend

MemoryOps needs to know **where your backend lives** before it can do anything.

1. Open **MemoryOps Settings** (the button below, or run `MemoryOps: Open Settings`).
2. Set **`memoryops.apiUrl`** to your backend's base URL — for local development this is usually `http://localhost:8080`.
3. Set **`memoryops.workspaceId`** to your workspace UUID.

> 💡 You can find your workspace UUID in the MemoryOps dashboard, or from the
> output of `mops workspace list` if you use the CLI.

Once the API URL and workspace ID are set, move on to authentication.
