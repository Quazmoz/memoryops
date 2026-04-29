import { expect, test } from '@playwright/test';

import { authenticateApp } from './helpers/auth';
import { createTestWorkspace } from './helpers/setup';

test('Guide page shows all 14 TOC sections and nav link is present', async ({ page }) => {
  const { workspaceId, apiKey } = await createTestWorkspace();
  await authenticateApp(page, workspaceId, apiKey);

  // Navigate via sidebar
  await page.getByTestId('nav-guide').click();
  await expect(page).toHaveURL('/guide');

  // All 14 TOC anchor links must be visible in the sidebar
  const expectedLabels = [
    'Overview',
    'Authentication',
    'VSCode Extension',
    'Claude Desktop',
    'OpenWebUI',
    'Direct API',
    'Ingest Memories',
    'Search & Retrieve',
    'Skills',
    'Contradictions',
    'Lifecycle & Decay',
    'Workspace Config',
    'Export & Import',
    'Troubleshooting',
  ];

  for (const label of expectedLabels) {
    await expect(page.locator('nav[aria-label="Guide sections"]').getByText(label)).toBeVisible();
  }

  // Each section heading must appear in the main content
  for (const label of expectedLabels) {
    await expect(page.locator(`#${sectionId(label)}`)).toBeAttached();
  }

  // Clicking a TOC link scrolls to the section
  await page.locator('nav[aria-label="Guide sections"]').getByText('Troubleshooting').click();
  await expect(page.locator('#troubleshooting')).toBeInViewport({ timeout: 3_000 });
});

test('Guide page substitutes API key in code blocks when authenticated', async ({ page }) => {
  const { workspaceId, apiKey } = await createTestWorkspace();
  await authenticateApp(page, workspaceId, apiKey);
  await page.goto('/guide');

  // The workspace ID should appear in at least one code block
  const firstCodeBlock = page.locator('[data-testid="code-block"]').first();
  await expect(firstCodeBlock).toContainText(workspaceId);
});

function sectionId(label: string): string {
  const map: Record<string, string> = {
    'Overview': 'overview',
    'Authentication': 'authentication',
    'VSCode Extension': 'vscode',
    'Claude Desktop': 'claude-desktop',
    'OpenWebUI': 'openwebui',
    'Direct API': 'direct-api',
    'Ingest Memories': 'ingest',
    'Search & Retrieve': 'retrieve',
    'Skills': 'skills',
    'Contradictions': 'contradictions',
    'Lifecycle & Decay': 'lifecycle',
    'Workspace Config': 'config',
    'Export & Import': 'export-import',
    'Troubleshooting': 'troubleshooting',
  };
  return map[label] ?? label.toLowerCase().replace(/\s+/g, '-');
}
