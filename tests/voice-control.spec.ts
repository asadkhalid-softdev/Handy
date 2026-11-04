import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    const callbacks = new Map<number, (event: unknown) => void>();
    const listeners = new Map<number, { event: string; handler: number }>();
    const calls: string[] = [];
    let id = 0;
    const status = {
      enabled: false,
      phase: "paused",
      phrase: "hey handy",
      silence_ms: 2000,
    };
    Object.assign(window, {
      testVoiceCalls: calls,
      testRejectConfigure: false,
      __TAURI_EVENT_PLUGIN_INTERNALS__: {
        unregisterListener: (_event: string, eventId: number) =>
          listeners.delete(eventId),
      },
      __TAURI_INTERNALS__: {
        metadata: { currentWindow: { label: "voice_control" } },
        transformCallback: (callback: (event: unknown) => void) => {
          callbacks.set(++id, callback);
          return id;
        },
        invoke: async (command: string, args: Record<string, unknown> = {}) => {
          calls.push(command);
          if (command === "plugin:event|listen") {
            listeners.set(++id, {
              event: String(args.event),
              handler: Number(args.handler),
            });
            return id;
          }
          if (command === "get_voice_control_status") return { ...status };
          if (command === "get_app_settings") return { app_language: "en" };
          if (command === "plugin:os|locale") return "en-US";
          if (command === "configure_voice_control") {
            if (Reflect.get(window, "testRejectConfigure"))
              throw new Error("Microphone unavailable");
            status.enabled = Boolean(args.enabled);
            status.phase = status.enabled ? "listening" : "paused";
            for (const entry of listeners.values()) {
              if (entry.event === "voice-control-status") {
                callbacks.get(entry.handler)?.({ payload: { ...status } });
              }
            }
          }
        },
      },
    });
  });
});

test("play and pause follow confirmed backend status", async ({ page }) => {
  await page.setViewportSize({ width: 316, height: 76 });
  await page.goto("/src/voice-control/index.html");
  await expect(page.getByText("Microphone paused")).toBeVisible();
  await page
    .getByRole("button", { name: "Resume microphone listening" })
    .click();
  await expect(page.getByText("Listening for wake phrase")).toBeVisible();
  await expect(page.getByText("Say “hey handy”")).toBeVisible();
  await page
    .getByRole("button", { name: "Pause microphone listening" })
    .click();
  await expect(page.getByText("Microphone paused")).toBeVisible();
});

test("a backend failure cannot falsely show microphone listening", async ({
  page,
}) => {
  await page.goto("/src/voice-control/index.html");
  await page.evaluate(() => Reflect.set(window, "testRejectConfigure", true));
  await page
    .getByRole("button", { name: "Resume microphone listening" })
    .click();
  await expect(page.getByText("Microphone paused")).toBeVisible();
  await expect(
    page.getByText("Check voice settings for details"),
  ).toBeVisible();
});

test("dragging and settings use their dedicated native actions", async ({
  page,
}) => {
  await page.goto("/src/voice-control/index.html");
  await page
    .getByRole("button", { name: "Drag to move voice control" })
    .hover();
  await page.mouse.down();
  await page.mouse.up();
  await page.getByRole("button", { name: "Open Handy settings" }).click();
  const calls = await page.evaluate(
    () => Reflect.get(window, "testVoiceCalls") as string[],
  );
  expect(calls).toContain("plugin:window|start_dragging");
  expect(calls).toContain("show_main_window_command");
  expect(calls).not.toContain("configure_voice_control");
});
