# Getting Started: First Workspace Bootstrap

Use this flow to create your first workspace and obtain the initial API key in one call.

## Create Workspace and API Key

`POST /v1/workspaces` does not require authentication. It creates the workspace and provisions one initial API key in the same response.

```bash
curl -sS -X POST http://localhost:8080/v1/workspaces \
  -H 'Content-Type: application/json' \
  -d '{"name":"my-first-workspace"}'
```

Example response:

```json
{
  "workspace_id": "0196f6c1-7e42-7f4f-8a6b-2945ea7f1e9a",
  "api_key": "mops_0196f6c1_xxxxxxxxxxxxxxxxxxxxxxxxx"
}
```

The response fields are:

| Field | Description |
|---|---|
| `workspace_id` | UUID for the newly created workspace. |
| `api_key` | Plaintext API key shown exactly once. |

## Use the Key

Send the returned key as either `Authorization: Bearer <api_key>` or `X-API-Key: <api_key>`:

```bash
curl -sS http://localhost:8080/v1/memory?workspace_id=0196f6c1-7e42-7f4f-8a6b-2945ea7f1e9a \
  -H 'Authorization: Bearer mops_0196f6c1_xxxxxxxxxxxxxxxxxxxxxxxxx'
```

## Important

The `api_key` value is only returned in plaintext once at creation time. Store it securely before closing the terminal.

## What's Next

- Connect **Open WebUI** with [docs/integrations/openwebui.md](integrations/openwebui.md).
- Connect **Claude Code** with [docs/integrations/claude-code.md](integrations/claude-code.md).
- Connect **VS Code, GitHub Copilot, or Continue.dev** with [docs/integrations/vscode.md](integrations/vscode.md).
- Create additional keys as needed with `POST /v1/workspaces/{id}/keys`.
- Configure workspace promotion, lifecycle, and memory-sharing settings through the workspace endpoints.
- See [docs/mcp-transport.md](mcp-transport.md) for the full MCP transport reference.
