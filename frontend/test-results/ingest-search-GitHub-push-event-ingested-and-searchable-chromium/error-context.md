# Instructions

- Following Playwright test failed.
- Explain why, be concise, respect Playwright best practices.
- Provide a snippet of code with the fix, if possible.

# Test info

- Name: ingest-search.spec.ts >> GitHub push event ingested and searchable
- Location: e2e\ingest-search.spec.ts:8:1

# Error details

```
Error: expect(locator).toBeVisible() failed

Locator: getByTestId('memory-result-row').first()
Expected: visible
Timeout: 15000ms
Error: element(s) not found

Call log:
  - Expect "toBeVisible" with timeout 15000ms
  - waiting for getByTestId('memory-result-row').first()

```

# Page snapshot

```yaml
- generic [ref=e3]:
  - complementary [ref=e4]:
    - generic [ref=e5]:
      - img [ref=e7]
      - generic [ref=e10]:
        - paragraph [ref=e11]: MemoryOps
        - paragraph [ref=e12]: Control Center
    - navigation "Primary" [ref=e13]:
      - link "Dashboard" [ref=e14] [cursor=pointer]:
        - /url: /
        - img [ref=e15]
        - generic [ref=e18]: Dashboard
      - link "Memory" [ref=e19] [cursor=pointer]:
        - /url: /memory
        - img [ref=e20]
        - generic [ref=e24]: Memory
      - link "Traces" [ref=e25] [cursor=pointer]:
        - /url: /trace
        - img [ref=e26]
        - generic [ref=e28]: Traces
      - link "Lifecycle" [ref=e29] [cursor=pointer]:
        - /url: /lifecycle
        - img [ref=e30]
        - generic [ref=e34]: Lifecycle
      - link "Ingest" [ref=e35] [cursor=pointer]:
        - /url: /ingest
        - img [ref=e36]
        - generic [ref=e39]: Ingest
      - link "Integrations" [ref=e40] [cursor=pointer]:
        - /url: /integrations
        - img [ref=e41]
        - generic [ref=e47]: Integrations
      - link "Audit" [ref=e48] [cursor=pointer]:
        - /url: /audit
        - img [ref=e49]
        - generic [ref=e52]: Audit
      - link "Settings" [ref=e53] [cursor=pointer]:
        - /url: /settings
        - img [ref=e54]
        - generic [ref=e57]: Settings
  - main [ref=e58]:
    - generic [ref=e59]:
      - generic [ref=e60]:
        - generic [ref=e61]:
          - paragraph [ref=e62]: Primary view
          - heading "Memory Explorer" [level=1] [ref=e63]
        - generic [ref=e64]:
          - generic [ref=e65]:
            - img
            - textbox "Search memory" [ref=e66]: push
          - button "Clear search" [ref=e67]:
            - img [ref=e68]
          - button "Search" [active] [ref=e71]:
            - img [ref=e72]
            - text: Search
      - generic [ref=e75]:
        - generic [ref=e76]:
          - generic [ref=e77]:
            - generic "Memory type filters" [ref=e78]:
              - button "all" [ref=e79]
              - button "episodic" [ref=e80]
              - button "semantic" [ref=e81]
            - button "Pinned" [ref=e82]:
              - img [ref=e83]
              - text: Pinned
            - generic [ref=e85]:
              - generic [ref=e86]:
                - generic [ref=e87]: Min importance
                - generic [ref=e88]: "0.00"
              - slider "Min importance 0.00" [ref=e89]: "0"
          - generic [ref=e90]:
            - generic [ref=e91]:
              - text: Agent ID
              - textbox "Agent ID" [ref=e92]
            - generic [ref=e93]:
              - text: User ID
              - textbox "User ID" [ref=e94]
            - generic [ref=e95]:
              - text: Repo
              - textbox "Repo" [ref=e96]:
                - /placeholder: owner/repo
        - generic [ref=e97]:
          - generic [ref=e98]:
            - text: Sort
            - combobox "Sort" [ref=e99]:
              - option "Importance" [selected]
              - option "Decay"
              - option "Updated"
              - option "Created"
          - button "desc" [ref=e100]:
            - img [ref=e101]
            - text: desc
      - generic [ref=e105]:
        - button "Tags" [ref=e106]:
          - generic [ref=e107]:
            - img [ref=e108]
            - text: Tags
          - img [ref=e111]
        - generic [ref=e114]: Loading tags
      - generic [ref=e115]:
        - generic [ref=e116]: Searching for
        - generic [ref=e117]: push
```

# Test source

```ts
  1  | // requires full stack
  2  | import { expect, test } from '@playwright/test';
  3  | 
  4  | import { authenticateApp } from './helpers/auth';
  5  | import { seedGitHubEvent } from './helpers/seed';
  6  | import { createTestWorkspace } from './helpers/setup';
  7  | 
  8  | test('GitHub push event ingested and searchable', async ({ page }) => {
  9  |   // 1. Create workspace + API key via API helpers
  10 |   const { workspaceId, apiKey } = await createTestWorkspace();
  11 | 
  12 |   // 2. Navigate to the app, enter workspace ID + API key in the first-run form
  13 |   await authenticateApp(page, workspaceId, apiKey);
  14 | 
  15 |   // 3. Navigate to Webhook Tester; fire a GitHub push event
  16 |   await page.getByTestId('nav-ingest').click();
  17 |   await page.getByTestId('source-tab-github').click();
  18 | 
  19 |   // Select the "push" event type
  20 |   await page.getByTestId('webhook-event-select').selectOption('push');
  21 | 
  22 |   // Wait for fixture effect to flush before firing webhook.
  23 |   await expect(page.getByTestId('fire-webhook-button')).toBeEnabled({ timeout: 5_000 });
  24 |   await expect(page.getByTestId('webhook-payload')).toContainText('refs/heads', { timeout: 5_000 });
  25 | 
  26 |   // Fire the webhook
  27 |   await page.getByTestId('fire-webhook-button').click();
  28 | 
  29 |   // Verify response status shows 202 (accepted)
  30 |   await expect(page.getByTestId('webhook-response-status')).toBeVisible({ timeout: 15_000 });
  31 |   await expect(page.getByTestId('webhook-response-status')).toContainText('202', { timeout: 5_000 });
  32 | 
  33 |   // 4. Seed a searchable memory via the real import API to avoid worker backlog.
  34 |   await seedGitHubEvent(workspaceId, apiKey);
  35 | 
  36 |   // 5. Navigate to Memory Explorer; search for a term in the seeded payload
  37 |   await page.getByTestId('nav-memory').click();
  38 |   await page.getByTestId('memory-search-input').fill('push');
  39 |   await page.getByTestId('memory-search-submit').click();
  40 | 
  41 |   // 6. At least one result row appears
> 42 |   await expect(page.getByTestId('memory-result-row').first()).toBeVisible({ timeout: 15_000 });
     |                                                               ^ Error: expect(locator).toBeVisible() failed
  43 | });
  44 | 
```