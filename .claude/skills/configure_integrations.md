# Skill: Configure Integrations

**Description:** Registers and configures tool integrations and webhook endpoints (GitHub, Slack, Jira, Linear) for a workspace.

## Trigger
Use this skill when:
- Connecting new engineering tool webhooks to MemoryOps.
- The user requests to setup integration credentials or update a webhook signing secret.

## Execution Steps
1. **Get Webhook Details**
   - Retrieve the ingest URL structure for your workspace: `http://localhost:8080/v1/ingest/<provider>/<workspace_id>`
2. **Register the Integration**
   - Perform a POST request to register the integration in MemoryOps and obtain/store the signing secret:
     - HTTP: `POST /v1/workspaces/{workspace_id}/integrations`
     - Payload: `{"provider": "github", "secret": "your-webhook-secret"}`
3. **Configure Tool Webhook**
   - Navigate to the tool settings (e.g. GitHub Repository Webhooks) and set Payload URL to the ingest URL and Content Type to `application/json`. Add the secret.
