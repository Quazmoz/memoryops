import { expect, test } from "@playwright/test";

import { authenticateApp } from "./helpers/auth";
import { createTestWorkspace } from "./helpers/setup";

test("dashboard help tooltip opens on hover", async ({ page }) => {
  const { workspaceId, apiKey } = await createTestWorkspace();
  await authenticateApp(page, workspaceId, apiKey);

  await page.getByTestId("nav-dashboard").click();
  await page.getByLabel("Help: Memory health").hover();

  await expect(
    page.getByText("Shows decay, deletion, and age signals so you can understand whether the workspace memory pool is stale, noisy, or healthy."),
  ).toBeVisible();
});
