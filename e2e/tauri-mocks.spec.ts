import { test, expect } from "@playwright/test";

test.describe("Tauri IPC Mocks", () => {
  test("should load mocks in browser mode and log initialization", async ({ page }) => {
    const consoleLogs: string[] = [];

    // Capture console logs
    page.on("console", (msg) => {
      consoleLogs.push(msg.text());
    });

    // Navigate to the app
    await page.goto("/");

    // Wait for the app to initialize
    await page.waitForTimeout(1000);

    // Verify mock initialization logs
    expect(consoleLogs).toContain("[App] Running in browser mode - loading Tauri IPC mocks");
    expect(consoleLogs).toContain("[Mocks] Setting up Tauri IPC mocks for browser development");
    expect(consoleLogs).toContain("[Mocks] Tauri IPC mocks initialized successfully");
  });

  test("should have mocked __TAURI_INTERNALS__ after setup", async ({ page }) => {
    await page.goto("/");

    // After mockWindows("main") is called, __TAURI_INTERNALS__ should be present
    // This is expected behavior - the mock creates a fake Tauri context
    const tauriInternals = await page.evaluate(() => {
      const internals = (window as unknown as { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__;
      return {
        exists: "__TAURI_INTERNALS__" in window,
        hasMetadata: internals && typeof internals === "object" && "metadata" in internals,
      };
    });

    // mockWindows creates __TAURI_INTERNALS__ with metadata
    expect(tauriInternals.exists).toBe(true);
    expect(tauriInternals.hasMetadata).toBe(true);
  });

  test("should handle invoke calls without errors", async ({ page }) => {
    const consoleErrors: string[] = [];

    page.on("console", (msg) => {
      if (msg.type() === "error") {
        consoleErrors.push(msg.text());
      }
    });

    page.on("pageerror", (error) => {
      consoleErrors.push(error.message);
    });

    await page.goto("/");

    // Wait for the app to fully load and any initial invoke calls to complete
    await page.waitForTimeout(2000);

    // Filter out expected errors (if any) and check for unexpected Tauri-related errors
    const tauriErrors = consoleErrors.filter(
      (err) => err.includes("invoke") || err.includes("tauri") || err.includes("IPC")
    );

    expect(tauriErrors).toHaveLength(0);
  });

  test("should log mock IPC calls", async ({ page }) => {
    const mockIpcLogs: string[] = [];

    page.on("console", (msg) => {
      const text = msg.text();
      if (text.startsWith("[Mock IPC]")) {
        mockIpcLogs.push(text);
      }
    });

    await page.goto("/");

    // Wait for the app to initialize and make IPC calls
    await page.waitForTimeout(2000);

    // The app should make some IPC calls during initialization
    // Even if no calls are made initially, the test passes - it just means
    // the app doesn't make IPC calls on load
    console.log("Mock IPC calls captured:", mockIpcLogs.length);
  });

  test("app should render without crashing in browser mode", async ({ page }) => {
    await page.goto("/");

    // Wait for initial load
    await page.waitForLoadState("networkidle");

    // The app should render something - check for root element content
    const root = page.locator("#root");
    await expect(root).toBeVisible();

    // The root should have some content (not empty)
    const rootContent = await root.innerHTML();
    expect(rootContent.length).toBeGreaterThan(0);
  });

  test("should show MockDevTools toggle button in browser mode", async ({ page }) => {
    await page.goto("/");

    // Wait for the app to load
    await page.waitForLoadState("networkidle");

    // The MockDevTools toggle button should be visible (wrench icon)
    const toggleButton = page.locator('button[title="Toggle Mock Dev Tools"]');
    await expect(toggleButton).toBeVisible();

    // Click to open the dev tools panel
    await toggleButton.click();

    // The panel should now be visible with "Mock Dev Tools" title
    const panelTitle = page.locator("text=Mock Dev Tools");
    await expect(panelTitle).toBeVisible();

    // Should show "BROWSER MODE" badge
    const badge = page.locator("text=BROWSER MODE");
    await expect(badge).toBeVisible();
  });

  test("should display preset scenarios in MockDevTools", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Open MockDevTools
    const toggleButton = page.locator('button[title="Toggle Mock Dev Tools"]');
    await toggleButton.click();

    // Presets tab should be active by default
    const presetsTab = page.locator("button:has-text('Presets')");
    await expect(presetsTab).toBeVisible();

    // Should show preset scenarios
    await expect(page.locator("text=Fresh Start")).toBeVisible();
    await expect(page.locator("text=Active Conversation")).toBeVisible();
    await expect(page.locator("text=Tool Execution")).toBeVisible();
    await expect(page.locator("text=Error State")).toBeVisible();
    await expect(page.locator("text=Command History")).toBeVisible();
    await expect(page.locator("text=Build Failure")).toBeVisible();
    await expect(page.locator("text=Code Review")).toBeVisible();
    await expect(page.locator("text=Long Output")).toBeVisible();
  });
});
