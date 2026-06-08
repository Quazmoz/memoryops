## Authenticate securely

MemoryOps requests are authenticated with a **workspace API key**.

Run **`MemoryOps: Set API Key (Secure)`** (the button below). Your key is stored
in your operating system's keychain via VS Code's **SecretStorage** — it is
**never** written to `settings.json` in plain text.

> 🔐 Prefer the secure command over the `memoryops.apiKey` setting. The plain
> setting exists only as a fallback for headless/CI scenarios and should be
> avoided on shared machines.

After storing your key, continue to verify the connection.
