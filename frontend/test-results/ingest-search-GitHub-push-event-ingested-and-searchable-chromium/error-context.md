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

Locator: getByTestId('webhook-response-status')
Expected: visible
Timeout: 15000ms
Error: element(s) not found

Call log:
  - Expect "toBeVisible" with timeout 15000ms
  - waiting for getByTestId('webhook-response-status')

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
        - paragraph [ref=e61]: Dev webhook console
        - heading "Webhook Tester" [level=1] [ref=e62]
      - generic [ref=e63]:
        - generic [ref=e64]:
          - generic [ref=e65]:
            - heading "Event" [level=3] [ref=e66]
            - tablist "Webhook source" [ref=e67]:
              - tab "GitHub" [selected] [ref=e68]
              - tab "Slack" [ref=e69]
              - tab "Linear" [ref=e70]
              - tab "Jira" [ref=e71]
          - generic [ref=e72]:
            - generic [ref=e73]:
              - text: Event type
              - combobox "Event type" [ref=e74]:
                - option "pull_request (opened)"
                - option "pull_request (merged)"
                - option "push" [selected]
                - option "pull_request_review (approved)"
                - option "issue"
                - option "issue_comment"
            - generic [ref=e75]:
              - paragraph [ref=e76]: Actor
              - paragraph [ref=e77]: nora
            - button "Fire Webhook" [ref=e78]:
              - img [ref=e79]
              - text: Fire Webhook
        - generic [ref=e82]:
          - heading "Payload" [level=3] [ref=e84]
          - textbox [ref=e86]: "{ \"ref\": \"refs/heads/main\", \"before\": \"9fceb02f9fceb02f9fceb02f9fceb02f9fceb02f\", \"after\": \"b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0\", \"pusher\": { \"name\": \"nora\", \"email\": \"nora@example.com\" }, \"repository\": { \"full_name\": \"Quazmoz/memoryops\", \"pushed_at\": 1777303230 }, \"commits\": [ { \"id\": \"b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0\", \"message\": \"Wire memory explorer filters\", \"timestamp\": \"2026-04-27T15:20:30Z\", \"author\": { \"name\": \"nora\", \"email\": \"nora@example.com\" } } ] }"
      - generic [ref=e87]:
        - generic [ref=e88]:
          - heading "Response" [level=3] [ref=e89]
          - generic [ref=e91]: github
        - generic [ref=e92]:
          - generic [ref=e93]:
            - paragraph [ref=e94]: Ready to fire
            - paragraph [ref=e95]: The selected fixture will send through the Vite proxy to the live backend.
          - generic [ref=e96]:
            - img [ref=e97]
            - generic [ref=e99]:
              - paragraph [ref=e100]: Something needs attention
              - paragraph [ref=e101]: signal is aborted without reason
```

# Test source

```ts
  1  | // requires full stack
  2  | import { expect, test } from '@playwright/test';
  3  | 
  4  | import { authenticateApp } from './helpers/auth';
  5  | import { waitForMemory } from './helpers/seed';
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
  22 |   // Fire the webhook
  23 |   await page.getByTestId('fire-webhook-button').click();
  24 | 
  25 |   // Verify response status shows 202 (accepted)
> 26 |   await expect(page.getByTestId('webhook-response-status')).toBeVisible({ timeout: 15_000 });
     |                                                             ^ Error: expect(locator).toBeVisible() failed
  27 |   await expect(page.getByTestId('webhook-response-status')).toContainText('202', { timeout: 10_000 });
  28 | 
  29 |   // 4. Wait for memory to appear (poll /v1/memory)
  30 |   await waitForMemory(workspaceId, apiKey);
  31 | 
  32 |   // 5. Navigate to Memory Explorer; search for a term in the seeded payload
  33 |   await page.getByTestId('nav-memory').click();
  34 |   await page.getByTestId('memory-search-input').fill('push');
  35 |   await page.getByTestId('memory-search-submit').click();
  36 | 
  37 |   // 6. At least one result row appears
  38 |   await expect(page.getByTestId('memory-result-row').first()).toBeVisible({ timeout: 15_000 });
  39 | });
  40 | 
```