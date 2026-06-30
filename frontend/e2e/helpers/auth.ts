// Shared E2E auth helper.
import { expect, type Page } from '@playwright/test';

const APP_STORE_STORAGE_KEY = 'memoryops-app-store';

/**
 * Authenticates the app by preloading the same persisted store state the
 * production FirstRunGate writes after entering a workspace.
 */
export async function authenticateApp(
  page: Page,
  workspaceId: string,
  apiKey: string,
): Promise<void> {
  await page.addInitScript(
    ({ storageKey, workspaceId: id, apiKey: key }) => {
      localStorage.setItem(
        storageKey,
        JSON.stringify({
          state: { workspaceId: id, apiKey: key },
          version: 0,
        }),
      );
    },
    { storageKey: APP_STORE_STORAGE_KEY, workspaceId, apiKey },
  );

  await page.goto('/');

  // Wait for the app shell to render (nav should appear)
  await page.getByTestId('nav-dashboard').waitFor({ state: 'visible', timeout: 15_000 });
  await page.getByTestId('nav-lifecycle').waitFor({ state: 'visible', timeout: 15_000 });
}
