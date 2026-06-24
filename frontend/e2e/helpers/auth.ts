// Shared E2E auth helper — authenticates via the FirstRunGate "Connect to existing" section
import { expect, type Page } from '@playwright/test';

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
  await page.getByRole('button', { name: 'Already have a workspace?' }).click();
  await page.getByTestId('api-key-input').fill(apiKey);

  const manualInput = page.getByTestId('workspace-id-input');
  const wsButton = page.locator(`button:has-text("${workspaceId.slice(0, 8)}")`);

  // Wait for the workspace selection button to appear. If it fails to load
  // (e.g. no workspaces found), fall back to entering the ID manually.
  try {
    await wsButton.waitFor({ state: 'visible', timeout: 5000 });
    await wsButton.click();
  } catch {
    await manualInput.fill(workspaceId);
  }

  const connectBtn = page.getByTestId('connect-button');
  await expect(connectBtn).toBeEnabled({ timeout: 5000 });
  await connectBtn.click();

  // Wait for the app shell to render (nav should appear)
  await page.getByTestId('nav-dashboard').waitFor({ state: 'visible', timeout: 15_000 });
  await page.getByTestId('nav-lifecycle').waitFor({ state: 'visible', timeout: 15_000 });
}
