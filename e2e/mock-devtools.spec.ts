import { test, expect } from "@playwright/test";

/**
 * MockDevTools E2E Tests
 *
 * These tests verify that mock events properly update the UI,
 * not just that events are dispatched. Each test triggers an action
 * and verifies that the expected content appears on screen.
 */

test.describe("MockDevTools - Preset UI Verification", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate and wait for app to fully load
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    // Wait for the session to be created
    await page.waitForTimeout(2000);
  });

  test("Fresh Start preset completes without error", async ({ page }) => {
    // Open MockDevTools
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();
    await expect(page.locator("text=Mock Dev Tools")).toBeVisible();

    // Click Fresh Start preset
    await page.locator("text=Fresh Start").click();

    // Wait for events to process
    await page.waitForTimeout(1000);

    // Note: Fresh Start only emits terminal_output without a command_start event,
    // so the output doesn't appear in the timeline (by design - the app only shows
    // output that is part of command blocks). Verify the preset completes by
    // checking the action log shows completion.
    await expect(page.locator("text=Fresh start complete")).toBeVisible({ timeout: 5000 });
  });

  test("Active Conversation preset displays terminal output and AI response", async ({ page }) => {
    // Open MockDevTools
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();

    // Click Active Conversation preset
    await page.locator("text=Active Conversation").click();

    // Wait for AI streaming to complete (has delays)
    await page.waitForTimeout(5000);

    // Verify terminal output from "cat src/main.rs" command is visible
    await expect(page.locator("text=Hello, world!")).toBeVisible({ timeout: 5000 });

    // Verify AI response text appears
    await expect(page.locator("text=basic Rust project")).toBeVisible({ timeout: 5000 });
  });

  test("Tool Execution preset displays tool request and result", async ({ page }) => {
    // Open MockDevTools
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();

    // Click Tool Execution preset
    await page.locator("text=Tool Execution").click();

    // Wait for preset to complete
    await page.waitForTimeout(3000);

    // Verify AI text appears
    await expect(page.locator("text=read the configuration file")).toBeVisible({ timeout: 5000 });

    // Verify tool request shows (tool name in UI)
    await expect(page.locator("text=read_file")).toBeVisible({ timeout: 5000 });

    // Verify tool result content is shown
    await expect(page.locator("text=Rust 2021 edition")).toBeVisible({ timeout: 5000 });
  });

  test("Error State preset displays error message", async ({ page }) => {
    // Open MockDevTools
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();

    // Click Error State preset
    await page.locator("text=Error State").click();

    // Wait for preset to complete
    await page.waitForTimeout(1500);

    // Verify error message appears
    await expect(page.locator("text=Rate limit exceeded")).toBeVisible({ timeout: 5000 });
  });

  test("Command History preset displays multiple command outputs", async ({ page }) => {
    // Open MockDevTools
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();

    // Click Command History preset
    await page.locator("text=Command History").click();

    // Wait for all 4 commands to complete (with delays)
    await page.waitForTimeout(4000);

    // Verify git status output
    await expect(page.locator("text=On branch main")).toBeVisible({ timeout: 5000 });

    // Verify cargo build output
    await expect(page.locator("text=Compiling my-app")).toBeVisible({ timeout: 5000 });

    // Verify cargo test output
    await expect(page.locator("text=test result: ok. 3 passed")).toBeVisible({ timeout: 5000 });
  });

  test("Build Failure preset displays compiler error and AI help", async ({ page }) => {
    // Open MockDevTools
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();

    // Click Build Failure preset
    await page.locator("text=Build Failure").click();

    // Wait for preset to complete (includes AI response)
    await page.waitForTimeout(6000);

    // Verify compiler error is shown
    await expect(page.locator("text=borrow of moved value")).toBeVisible({ timeout: 5000 });

    // Verify AI help response appears
    await expect(page.locator("text=borrow checker error")).toBeVisible({ timeout: 5000 });
  });

  test("Code Review preset displays code and review comments", async ({ page }) => {
    // Open MockDevTools
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();

    // Click Code Review preset
    await page.locator("text=Code Review").click();

    // Wait for preset to complete (includes AI review streaming)
    await page.waitForTimeout(7000);

    // Verify the code being reviewed is shown (use specific text to avoid duplicates)
    await expect(page.locator("text=cat src/handlers.rs").first()).toBeVisible({ timeout: 5000 });

    // Verify review comments appear (check for a specific review point)
    await expect(page.locator("text=anti-pattern")).toBeVisible({ timeout: 5000 });
  });

  test("Long Output preset displays extensive test output", async ({ page }) => {
    // Open MockDevTools
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();

    // Click Long Output preset
    await page.locator("text=Long Output").click();

    // Wait for events to process
    await page.waitForTimeout(2000);

    // Verify test output header appears
    await expect(page.locator("text=running 50 tests")).toBeVisible({ timeout: 5000 });

    // Verify doc test output also appears
    await expect(page.locator("text=Doc-tests my-app")).toBeVisible({ timeout: 5000 });
  });
});

test.describe("MockDevTools - Terminal Tab UI Verification", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(2000);

    // Open MockDevTools and switch to Terminal tab
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();
    await expect(page.locator("text=Mock Dev Tools")).toBeVisible();
    await page.locator("button:has-text('Terminal')").click();
  });

  test("Emit Output button displays terminal output in timeline", async ({ page }) => {
    // Set custom output text
    const customOutput = "Custom terminal output for testing\n";
    await page.locator("textarea").first().fill(customOutput);

    // Click Emit Output
    await page.locator("button:has-text('Emit Output')").click();

    // Wait for event processing
    await page.waitForTimeout(1000);

    // Verify the output appears in the UI
    await expect(page.locator("text=Custom terminal output for testing")).toBeVisible({ timeout: 5000 });
  });

  test("Emit Command Block displays command with output in timeline", async ({ page }) => {
    // Set custom command and output
    const commandInput = page.locator('input[type="text"]').nth(1); // Command input
    await commandInput.fill("echo 'test command'");

    const outputTextarea = page.locator("textarea").last();
    await outputTextarea.fill("Command output result");

    // Click Emit Command Block
    await page.locator("button:has-text('Emit Command Block')").click();

    // Wait for event processing
    await page.waitForTimeout(1000);

    // Verify command appears in timeline (use first() since it may appear in multiple places)
    await expect(page.locator("text=echo 'test command'").first()).toBeVisible({ timeout: 5000 });

    // Verify output appears in timeline
    await expect(page.locator("text=Command output result").first()).toBeVisible({ timeout: 5000 });
  });
});

test.describe("MockDevTools - AI Tab UI Verification", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(2000);

    // Open MockDevTools and switch to AI tab
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();
    await expect(page.locator("text=Mock Dev Tools")).toBeVisible();
    await page.locator("button:has-text('AI')").click();
  });

  test("Simulate Response displays streamed AI text in timeline", async ({ page }) => {
    // Set custom AI response
    const customResponse = "This is a custom AI response for testing the streaming feature.";
    await page.locator("textarea").first().fill(customResponse);

    // Set a fast stream delay
    await page.locator('input[type="number"]').first().fill("10");

    // Click Simulate Response
    await page.locator("button:has-text('Simulate Response')").click();

    // Wait for streaming to complete
    await page.waitForTimeout(3000);

    // Verify the AI response text appears in the UI
    await expect(page.locator("text=custom AI response for testing")).toBeVisible({ timeout: 5000 });
  });

  test("Emit Tool Request displays tool card in timeline", async ({ page }) => {
    // Set tool name and args
    const toolNameInput = page.locator('input[type="text"]').first();
    await toolNameInput.fill("write_file");

    const toolArgsTextarea = page.locator("textarea").last();
    await toolArgsTextarea.fill('{"path": "/test/file.txt", "content": "hello"}');

    // Click Emit Tool Request
    await page.locator("button:has-text('Emit Tool Request')").click();

    // Wait for event processing
    await page.waitForTimeout(1000);

    // Verify tool request card appears with tool name (use first() since it may appear in log too)
    await expect(page.locator("text=write_file").first()).toBeVisible({ timeout: 5000 });
  });

  test("Emit Error displays error message in timeline", async ({ page }) => {
    // Click Emit Error
    await page.locator("button:has-text('Emit Error')").click();

    // Wait for event processing
    await page.waitForTimeout(1000);

    // Verify error message appears
    await expect(page.locator("text=Mock error for testing")).toBeVisible({ timeout: 5000 });
  });
});

test.describe("MockDevTools - Session Tab Functionality", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");
    await page.waitForTimeout(2000);

    // Open MockDevTools and switch to Session tab
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();
    await expect(page.locator("text=Mock Dev Tools")).toBeVisible();
    await page.locator("button:has-text('Session')").click();
  });

  test("New Session ID button generates a new session ID", async ({ page }) => {
    // Get the initial session ID
    const sessionInput = page.locator('input[type="text"]').first();
    const initialValue = await sessionInput.inputValue();

    // Click New Session ID button
    await page.locator("button:has-text('New Session ID')").click();

    // Wait for update
    await page.waitForTimeout(200);

    // Verify the session ID changed
    const newValue = await sessionInput.inputValue();
    expect(newValue).not.toBe(initialValue);
    expect(newValue).toContain("mock-session-");
  });

  test("Session ID input can be changed manually", async ({ page }) => {
    const sessionInput = page.locator('input[type="text"]').first();
    const customSessionId = "custom-session-123";

    // Clear and fill with custom session ID
    await sessionInput.fill(customSessionId);

    // Verify the value is set
    const value = await sessionInput.inputValue();
    expect(value).toBe(customSessionId);
  });
});

test.describe("MockDevTools - Panel Interaction", () => {
  test("Toggle button opens and closes the panel", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Panel should be closed initially
    await expect(page.locator("text=Mock Dev Tools")).not.toBeVisible();

    // Click toggle button to open
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();
    await expect(page.locator("text=Mock Dev Tools")).toBeVisible();

    // Click toggle button to close
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();
    await expect(page.locator("text=Mock Dev Tools")).not.toBeVisible();
  });

  test("Tab navigation works correctly", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Open panel
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();

    // Verify Presets tab is active by default (shows Scenarios section)
    await expect(page.locator("text=Scenarios")).toBeVisible();

    // Switch to Terminal tab
    await page.locator("button:has-text('Terminal')").click();
    await expect(page.locator("text=Terminal Output")).toBeVisible();

    // Switch to AI tab
    await page.locator("button:has-text('AI')").click();
    await expect(page.locator("text=Streaming Response")).toBeVisible();

    // Switch to Session tab
    await page.locator("button:has-text('Session')").click();
    await expect(page.locator("text=Session Management")).toBeVisible();

    // Switch back to Presets
    await page.locator("button:has-text('Presets')").click();
    await expect(page.locator("text=Scenarios")).toBeVisible();
  });

  test("All preset cards are visible in the Presets tab", async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Open panel
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();

    // Verify all 8 presets are listed
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
