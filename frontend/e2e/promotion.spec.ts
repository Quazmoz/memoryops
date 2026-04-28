// requires full stack
import { expect, test } from '@playwright/test';

import { authenticateApp } from './helpers/auth';
import { seedGitHubEvent, waitForMemory } from './helpers/seed';
import { createTestWorkspace } from './helpers/setup';

test('Manual workspace promotion trigger completes', async ({ page }) => {
  // Setup: create workspace, seed a memory, wait for it to be indexed
  const { workspaceId, apiKey } = await createTestWorkspace();
  await seedGitHubEvent(workspaceId, apiKey);
  await waitForMemory(workspaceId, apiKey);

  // Navigate to app and authenticate
  await authenticateApp(page, workspaceId, apiKey);

  // Navigate to Lifecycle / Promotion view
  await page.getByTestId('nav-lifecycle').click();

  // Wait for the button to be enabled (workspace query must have resolved)
  await expect(page.getByTestId('manual-promote-button')).toBeEnabled({ timeout: 10_000 });

  // Click manual promotion trigger button
  await page.getByTestId('manual-promote-button').click();

  // Expect the promotion report to appear (not an error)
  await expect(page.getByTestId('promote-status')).toBeVisible({ timeout: 30_000 });
  await expect(page.getByTestId('promote-status')).not.toContainText('error', { ignoreCase: true });
});
