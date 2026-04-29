// requires full stack
import { expect, test } from '@playwright/test';

import { authenticateApp } from './helpers/auth';
import { seedGitHubEvent, waitForMemory } from './helpers/seed';
import { createTestWorkspace } from './helpers/setup';

test('GitHub push event ingested and searchable', async ({ page }) => {
  // 1. Create workspace + API key via API helpers
  const { workspaceId, apiKey } = await createTestWorkspace();

  // 2. Navigate to the app, enter workspace ID + API key in the first-run form
  await authenticateApp(page, workspaceId, apiKey);

  // 3. Navigate to Webhook Tester; fire a GitHub push event
  await page.getByTestId('nav-ingest').click();
  await page.getByTestId('source-tab-github').click();

  // Select the "push" event type
  await page.getByTestId('webhook-event-select').selectOption('push');

  // Wait for fixture effect to flush before firing webhook.
  await expect(page.getByTestId('fire-webhook-button')).toBeEnabled({ timeout: 5_000 });
  await expect(page.getByTestId('webhook-payload')).toContainText('refs/heads', { timeout: 5_000 });

  // Fire the webhook
  await page.getByTestId('fire-webhook-button').click();

  // Verify response status shows 202 (accepted)
  await expect(page.getByTestId('webhook-response-status')).toBeVisible({ timeout: 15_000 });
  await expect(page.getByTestId('webhook-response-status')).toContainText('202', { timeout: 5_000 });

  // 4. Seed a searchable memory via the real import API to avoid worker backlog,
  //    then wait for the row to be queryable before searching. Without this poll
  //    the search fires before the committed row is visible to the retrieval query.
  await seedGitHubEvent(workspaceId, apiKey);
  await waitForMemory(workspaceId, apiKey);

  // 5. Navigate to Memory Explorer; search for a term in the seeded payload
  await page.getByTestId('nav-memory').click();
  await page.getByTestId('memory-search-input').fill('push');
  await page.getByTestId('memory-search-submit').click();

  // 6. At least one result row appears
  await expect(page.getByTestId('memory-result-row').first()).toBeVisible({ timeout: 15_000 });
});
