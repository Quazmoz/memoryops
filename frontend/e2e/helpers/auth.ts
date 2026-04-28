// Shared E2E auth helper — authenticates via the FirstRunGate "Connect to existing" section
import type { Page } from '@playwright/test';

/**
 * Authenticates the app by filling the workspace-id and api-key inputs
 * in the FirstRunGate "Connect to existing" section, then clicking Connect.
 */
export async function authenticateApp(
  page: Page,
  workspaceId: string,
  apiKey: string,
): Promise<void> {
  await page.goto('/');
  await page.getByTestId('workspace-id-input').fill(workspaceId);
  await page.getByTestId('api-key-input').fill(apiKey);
  await page.getByTestId('connect-button').click();

  // Wait for the app shell to render (nav should appear)
  await page.getByTestId('nav-dashboard').waitFor({ state: 'visible', timeout: 10_000 });
  await page.getByTestId('nav-lifecycle').waitFor({ state: 'visible', timeout: 15_000 });
}
