import { test, expect } from "@playwright/test";

test.describe("MockDevTools Presets", () => {
  test.beforeEach(async ({ page }) => {
    // Navigate and wait for app to load
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Open MockDevTools
    const toggleButton = page.locator('button[title="Toggle Mock Dev Tools"]');
    await expect(toggleButton).toBeVisible();
    await toggleButton.click();

    // Ensure panel is open
    await expect(page.locator("text=Mock Dev Tools")).toBeVisible();
  });

  test("Fresh Start preset emits terminal output", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Fresh Start preset
    await page.locator("text=Fresh Start").click();

    // Wait for preset to complete
    await page.waitForTimeout(1000);

    // Verify terminal_output events were dispatched
    const terminalOutputEvents = eventLogs.filter((log) =>
      log.includes('"terminal_output"')
    );
    expect(terminalOutputEvents.length).toBeGreaterThan(0);
  });

  test("Active Conversation preset emits command and AI events", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Active Conversation preset
    await page.locator("text=Active Conversation").click();

    // Wait for preset to complete (includes delays)
    await page.waitForTimeout(4000);

    // Verify command_block events were dispatched
    const commandBlockEvents = eventLogs.filter((log) =>
      log.includes('"command_block"')
    );
    expect(commandBlockEvents.length).toBeGreaterThan(0);

    // Verify AI events were dispatched
    const aiEvents = eventLogs.filter((log) => log.includes('"ai-event"'));
    expect(aiEvents.length).toBeGreaterThan(0);

    // Should have started, text_delta, and completed events
    const hasStarted = eventLogs.some((log) => log.includes("started"));
    const hasTextDelta = eventLogs.some((log) => log.includes("text_delta"));
    const hasCompleted = eventLogs.some((log) => log.includes("completed"));

    expect(hasStarted).toBe(true);
    expect(hasTextDelta).toBe(true);
    expect(hasCompleted).toBe(true);
  });

  test("Tool Execution preset emits tool request and result events", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Tool Execution preset
    await page.locator("text=Tool Execution").click();

    // Wait for preset to complete
    await page.waitForTimeout(3000);

    // Verify tool_request events
    const hasToolRequest = eventLogs.some((log) => log.includes("tool_request"));
    expect(hasToolRequest).toBe(true);

    // Verify tool_result events
    const hasToolResult = eventLogs.some((log) => log.includes("tool_result"));
    expect(hasToolResult).toBe(true);
  });

  test("Error State preset emits error event", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Error State preset
    await page.locator("text=Error State").click();

    // Wait for preset to complete
    await page.waitForTimeout(1500);

    // Verify ai-event was dispatched (error is a type of ai-event)
    const aiEvents = eventLogs.filter((log) => log.includes('"ai-event"'));
    expect(aiEvents.length).toBeGreaterThan(0);
  });

  test("Command History preset emits multiple commands", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Command History preset
    await page.locator("text=Command History").click();

    // Wait for preset to complete (4 commands with delays)
    await page.waitForTimeout(3000);

    // Verify multiple command_block events were dispatched
    const commandBlockEvents = eventLogs.filter((log) =>
      log.includes('"command_block"')
    );
    // Should have command_start and command_end for each of 4 commands = 8 events
    expect(commandBlockEvents.length).toBeGreaterThanOrEqual(8);

    // Verify terminal output events
    const terminalOutputEvents = eventLogs.filter((log) =>
      log.includes('"terminal_output"')
    );
    expect(terminalOutputEvents.length).toBeGreaterThan(0);
  });

  test("Build Failure preset emits failed command and AI response", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Build Failure preset
    await page.locator("text=Build Failure").click();

    // Wait for preset to complete
    await page.waitForTimeout(5000);

    // Verify command events were dispatched
    const hasCommandBlock = eventLogs.some((log) =>
      log.includes('"command_block"')
    );
    expect(hasCommandBlock).toBe(true);

    // Verify AI response events
    const aiEvents = eventLogs.filter((log) => log.includes('"ai-event"'));
    expect(aiEvents.length).toBeGreaterThan(0);
  });

  test("Code Review preset emits command and review", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Code Review preset
    await page.locator("text=Code Review").click();

    // Wait for preset to complete
    await page.waitForTimeout(6000);

    // Verify command_block events
    const hasCommandBlock = eventLogs.some((log) =>
      log.includes('"command_block"')
    );
    expect(hasCommandBlock).toBe(true);

    // Verify AI review events
    const aiEvents = eventLogs.filter((log) => log.includes('"ai-event"'));
    expect(aiEvents.length).toBeGreaterThan(0);
  });

  test("Long Output preset emits large terminal output", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Long Output preset
    await page.locator("text=Long Output").click();

    // Wait for preset to complete
    await page.waitForTimeout(2000);

    // Verify terminal_output events were dispatched
    const terminalOutputEvents = eventLogs.filter((log) =>
      log.includes('"terminal_output"')
    );
    expect(terminalOutputEvents.length).toBeGreaterThan(0);

    // Verify command_block events
    const commandBlockEvents = eventLogs.filter((log) =>
      log.includes('"command_block"')
    );
    expect(commandBlockEvents.length).toBeGreaterThanOrEqual(2); // command_start and command_end
  });
});

test.describe("MockDevTools Terminal Tab", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Open MockDevTools
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();
    await expect(page.locator("text=Mock Dev Tools")).toBeVisible();

    // Switch to Terminal tab
    await page.locator("button:has-text('Terminal')").click();
  });

  test("Emit Output button dispatches terminal_output event", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Emit Output button
    await page.locator("button:has-text('Emit Output')").click();

    // Wait for event
    await page.waitForTimeout(500);

    // Verify terminal_output event was dispatched
    const hasTerminalOutput = eventLogs.some((log) =>
      log.includes('"terminal_output"')
    );
    expect(hasTerminalOutput).toBe(true);
  });

  test("Emit Command Block button dispatches command events", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Emit Command Block button
    await page.locator("button:has-text('Emit Command Block')").click();

    // Wait for events
    await page.waitForTimeout(500);

    // Verify command_block events were dispatched
    const commandBlockEvents = eventLogs.filter((log) =>
      log.includes('"command_block"')
    );
    expect(commandBlockEvents.length).toBeGreaterThanOrEqual(2); // command_start and command_end

    // Verify terminal_output events
    const terminalOutputEvents = eventLogs.filter((log) =>
      log.includes('"terminal_output"')
    );
    expect(terminalOutputEvents.length).toBeGreaterThan(0);
  });

  test("Emit Directory Changed button dispatches directory_changed event", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Emit Directory Changed button
    await page.locator("button:has-text('Emit Directory Changed')").click();

    // Wait for event
    await page.waitForTimeout(500);

    // Verify directory_changed event was dispatched
    const hasDirectoryChanged = eventLogs.some((log) =>
      log.includes('"directory_changed"')
    );
    expect(hasDirectoryChanged).toBe(true);
  });

  test("Custom terminal output can be set and emitted", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Find the Output Data textarea and set custom content
    const outputTextarea = page.locator('textarea').first();
    await outputTextarea.fill("Custom test output\n");

    // Click Emit Output
    await page.locator("button:has-text('Emit Output')").click();

    // Wait for event
    await page.waitForTimeout(500);

    // Verify the event was dispatched
    const hasTerminalOutput = eventLogs.some((log) =>
      log.includes('"terminal_output"')
    );
    expect(hasTerminalOutput).toBe(true);
  });
});

test.describe("MockDevTools AI Tab", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Open MockDevTools
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();
    await expect(page.locator("text=Mock Dev Tools")).toBeVisible();

    // Switch to AI tab
    await page.locator("button:has-text('AI')").click();
  });

  test("Simulate Response button dispatches AI streaming events", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Simulate Response button
    await page.locator("button:has-text('Simulate Response')").click();

    // Wait for streaming to complete (default text with delays)
    await page.waitForTimeout(3000);

    // Verify multiple ai-event dispatches happened (started, text_deltas, completed)
    const aiEvents = eventLogs.filter((log) => log.includes('"ai-event"'));
    // Should have at least: 1 started + multiple text_deltas + 1 completed
    expect(aiEvents.length).toBeGreaterThan(3);
  });

  test("Emit Tool Request button dispatches tool_request event", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Emit Tool Request button
    await page.locator("button:has-text('Emit Tool Request')").click();

    // Wait for event
    await page.waitForTimeout(500);

    // Verify ai-event was dispatched (tool_request is a type of ai-event)
    const aiEvents = eventLogs.filter((log) => log.includes('"ai-event"'));
    expect(aiEvents.length).toBeGreaterThan(0);
  });

  test("Emit Tool Result button dispatches tool_result event", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Emit Tool Result button
    await page.locator("button:has-text('Emit Tool Result')").click();

    // Wait for event
    await page.waitForTimeout(500);

    // Verify ai-event was dispatched (tool_result is a type of ai-event)
    const aiEvents = eventLogs.filter((log) => log.includes('"ai-event"'));
    expect(aiEvents.length).toBeGreaterThan(0);
  });

  test("Emit Error button dispatches error event", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click Emit Error button
    await page.locator("button:has-text('Emit Error')").click();

    // Wait for event
    await page.waitForTimeout(500);

    // Verify ai-event was dispatched (error is a type of ai-event)
    const aiEvents = eventLogs.filter((log) => log.includes('"ai-event"'));
    expect(aiEvents.length).toBeGreaterThan(0);
  });
});

test.describe("MockDevTools Session Tab", () => {
  test.beforeEach(async ({ page }) => {
    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Open MockDevTools
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();
    await expect(page.locator("text=Mock Dev Tools")).toBeVisible();

    // Switch to Session tab
    await page.locator("button:has-text('Session')").click();
  });

  test("End Session button dispatches session_ended event", async ({ page }) => {
    const eventLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("[Mock Events] Dispatching")) {
        eventLogs.push(text);
      }
    });

    // Click End Session button
    await page.locator("button:has-text('End Session')").click();

    // Wait for event
    await page.waitForTimeout(500);

    // Verify session_ended event was dispatched
    const hasSessionEnded = eventLogs.some((log) =>
      log.includes('"session_ended"')
    );
    expect(hasSessionEnded).toBe(true);
  });

  test("New Session ID button generates new ID", async ({ page }) => {
    // Get the initial session ID
    const sessionInput = page.locator('input[type="text"]').first();
    const initialValue = await sessionInput.inputValue();

    // Click New Session ID button
    await page.locator("button:has-text('New Session ID')").click();

    // Wait for update
    await page.waitForTimeout(100);

    // Verify the session ID changed
    const newValue = await sessionInput.inputValue();
    expect(newValue).not.toBe(initialValue);
    expect(newValue).toContain("mock-session-");
  });
});

test.describe("Event Listeners Receive Dispatched Events", () => {
  test("terminal_output events are received by listeners", async ({ page }) => {
    const listenerLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      // Check for dispatching to listeners (not "No listeners")
      if (text.includes("Dispatching") && text.includes("listener(s)")) {
        listenerLogs.push(text);
      }
    });

    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Wait for listeners to register
    await page.waitForTimeout(2000);

    // Open MockDevTools and emit terminal output
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();
    await page.locator("button:has-text('Terminal')").click();
    await page.locator("button:has-text('Emit Output')").click();

    // Wait for event processing
    await page.waitForTimeout(500);

    // Verify events were dispatched to listeners (not "No listeners")
    const terminalListenerCalls = listenerLogs.filter((log) =>
      log.includes('"terminal_output"')
    );
    expect(terminalListenerCalls.length).toBeGreaterThan(0);
  });

  test("ai-event events are received by listeners", async ({ page }) => {
    const listenerLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("Dispatching") && text.includes("listener(s)")) {
        listenerLogs.push(text);
      }
    });

    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Wait for listeners to register
    await page.waitForTimeout(2000);

    // Open MockDevTools and emit AI event
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();
    await page.locator("button:has-text('AI')").click();
    await page.locator("button:has-text('Emit Error')").click();

    // Wait for event processing
    await page.waitForTimeout(500);

    // Verify events were dispatched to listeners
    const aiListenerCalls = listenerLogs.filter((log) =>
      log.includes('"ai-event"')
    );
    expect(aiListenerCalls.length).toBeGreaterThan(0);
  });

  test("command_block events are received by listeners", async ({ page }) => {
    const listenerLogs: string[] = [];
    page.on("console", (msg) => {
      const text = msg.text();
      if (text.includes("Dispatching") && text.includes("listener(s)")) {
        listenerLogs.push(text);
      }
    });

    await page.goto("/");
    await page.waitForLoadState("networkidle");

    // Wait for listeners to register
    await page.waitForTimeout(2000);

    // Open MockDevTools and emit command block
    await page.locator('button[title="Toggle Mock Dev Tools"]').click();
    await page.locator("button:has-text('Terminal')").click();
    await page.locator("button:has-text('Emit Command Block')").click();

    // Wait for event processing
    await page.waitForTimeout(500);

    // Verify events were dispatched to listeners
    const commandListenerCalls = listenerLogs.filter((log) =>
      log.includes('"command_block"')
    );
    expect(commandListenerCalls.length).toBeGreaterThan(0);
  });
});
