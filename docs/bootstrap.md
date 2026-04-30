# Getting Started: First Workspace Bootstrap

Use this flow to create your first workspace and obtain an API key.

## Create Workspace (No Auth)

Endpoint:

- `POST /v1/workspaces`

This bootstrap endpoint does not require authentication. It creates a workspace and provisions one initial API key in the same response.

Response shape:

- `workspace_id`: UUID for the newly created workspace
- `api_key`: plaintext API key (shown exactly once)

Example:

```bash
curl -sS -X POST http://localhost:3000/v1/workspaces \
  -H "Content-Type: application/json" \
  -d '{"name":"my-first-workspace"}'
```

Example response:

```json
{
  "workspace_id": "0196f6c1-7e42-7f4f-8a6b-2945ea7f1e9a",
  "api_key": "mops_0196f6c1_xxxxxxxxxxxxxxxxxxxxxxxxx"
}
```

## Important

The `api_key` value is only returned in plaintext once at creation time. Store it securely.

## What To Do Next

1. Use the returned key as `Authorization: Bearer <api_key>` or `X-API-Key` for API calls.
2. Create additional keys as needed via `POST /v1/workspaces/{id}/keys`.
3. Configure workspace settings (for example promotion and lifecycle options) via workspace config endpoints.
