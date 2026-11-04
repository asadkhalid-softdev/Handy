import { expect, test } from "@playwright/test";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => {
    const callbacks = new Map<number, (event: unknown) => void>();
    const listeners = new Map<number, { event: string; handler: number }>();
    const calls: string[] = [];
    let id = 0;
    const status = {
      feature_enabled: true,
      enabled: false,
      phase: "paused",
      phrase: "hey handy",
      silence_ms: 2000,
    };
    const emit = (event: string, payload: unknown) => {
      for (const entry of listeners.values()) {
        if (entry.event === event) callbacks.get(entry.handler)?.({ payload });
      }
    };
    const configurations: Record<string, unknown>[] = [];
    const settings = {
      app_language: "en",
      theme: "dark",
      onboarding_completed: true,
      selected_model: "test-english",
      spoken_languages: ["en"],
      selected_language: "en",
      bindings: {},
      debug_mode: false,
      post_process_enabled: false,
      show_whats_new_on_update: false,
      update_checks_enabled: false,
      custom_words: [],
      voice_control: status,
    };
    Object.assign(window, {
      __TAURI_OS_PLUGIN_INTERNALS__: {
        os_type: "windows",
        platform: "windows",
        family: "windows",
        arch: "x86_64",
        eol: "\r\n",
      },
      testVoiceConfigurations: configurations,
      testVoiceEmit: emit,
      testVoiceCalls: calls,
      testRejectConfigure: false,
      __TAURI_EVENT_PLUGIN_INTERNALS__: {
        unregisterListener: (_event: string, eventId: number) =>
          listeners.delete(eventId),
      },
      __TAURI_INTERNALS__: {
        metadata: {
          currentWindow: { label: "voice_control" },
          currentWebview: { label: "voice_control" },
        },
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
          if (
            command === "get_app_settings" ||
            command === "get_default_settings"
          )
            return settings;
          if (command === "get_windows_microphone_permission_status")
            return { supported: true, overall_access: "allowed" };
          if (command === "plugin:app|version") return "0.10.0";
          if (command === "get_current_model") return "test-english";
          if (command === "plugin:os|locale") return "en-US";
          if (command === "configure_voice_control") {
            if (Reflect.get(window, "testRejectConfigure"))
              throw new Error("Microphone unavailable");
            configurations.push(args);
            status.feature_enabled = Boolean(args.featureEnabled);
            status.enabled = Boolean(args.enabled) && status.feature_enabled;
            status.phrase = String(args.phrase);
            status.silence_ms = Number(args.silenceMs);
            status.phase = !status.feature_enabled
              ? "disabled"
              : status.enabled
                ? "listening"
                : "paused";
            emit("voice-control-status", { ...status });
            return;
          }
          return [];
        },
      },
    });
  });
});

test("play and pause follow confirmed backend status", async ({
  page,
}, testInfo) => {
  await page.setViewportSize({ width: 128, height: 58 });
  await page.goto("/src/voice-control/index.html");
  await expect(page.locator(".voice-panel")).toHaveClass(/paused/);
  await page
    .getByRole("button", { name: "Resume microphone listening" })
    .click();
  await expect(page.locator(".voice-panel")).toHaveClass(/listening/);
  await expect(page.getByRole("button")).toHaveCount(3);
  await expect(page.locator(".voice-copy, .voice-mic")).toHaveCount(0);
  await expect(page.getByRole("status")).toHaveCSS("clip-path", "inset(50%)");
  await page.screenshot({ path: testInfo.outputPath("floating-icons.png") });
  await page
    .getByRole("button", { name: "Pause microphone listening" })
    .click();
  await expect(page.locator(".voice-panel")).toHaveClass(/paused/);
});

test("a backend failure cannot falsely show microphone listening", async ({
  page,
}) => {
  await page.goto("/src/voice-control/index.html");
  await page.evaluate(() => Reflect.set(window, "testRejectConfigure", true));
  await page
    .getByRole("button", { name: "Resume microphone listening" })
    .click();
  await expect(page.locator(".voice-panel")).toHaveClass(/paused/);
  await expect(page.locator(".voice-panel")).toHaveClass(/has-error/);
  await expect(
    page.getByRole("button", { name: "Open Wake-Talk settings" }),
  ).toHaveAttribute("title", "Check Wake-Talk settings for details");
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
  await page.getByRole("button", { name: "Open Wake-Talk settings" }).click();
  const calls = await page.evaluate(
    () => Reflect.get(window, "testVoiceCalls") as string[],
  );
  expect(calls).toContain("plugin:window|start_dragging");
  expect(calls).toContain("open_wake_talk");
  expect(calls).not.toContain("configure_voice_control");
});

test("Wake-Talk has separate feature, listening and wake-word controls", async ({
  page,
}, testInfo) => {
  await page.goto("/");
  await page.getByText("Wake-Talk", { exact: true }).click();
  await expect(
    page.getByRole("heading", { name: "Wake-Talk", exact: true }),
  ).toBeVisible();
  await page
    .getByRole("button", { name: "Resume microphone listening" })
    .click();
  await expect(
    page.getByRole("button", { name: "Pause microphone listening" }),
  ).toBeVisible();
  await expect(page.getByLabel("Wake word or phrase")).toBeDisabled();
  await page.getByRole("switch", { name: "Enable Wake-Talk" }).uncheck();
  await expect(
    page.getByText("Wake-Talk disabled", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByRole("button", { name: "Resume microphone listening" }),
  ).toBeDisabled();
  await page.getByLabel("Wake word or phrase").fill("computer");
  await page.getByRole("button", { name: "Save settings" }).click();
  const saved = await page.evaluate(
    () =>
      Reflect.get(window, "testVoiceConfigurations") as Record<
        string,
        unknown
      >[],
  );
  expect(saved[saved.length - 1]).toMatchObject({
    featureEnabled: false,
    enabled: false,
    phrase: "computer",
  });
  await page.getByRole("switch", { name: "Enable Wake-Talk" }).check();
  await expect(
    page.getByRole("button", { name: "Resume microphone listening" }),
  ).toBeEnabled();
  await page.screenshot({
    path: testInfo.outputPath("wake-talk-settings.png"),
  });
});

test("floating settings navigation selects Wake-Talk in the app", async ({
  page,
}) => {
  await page.goto("/");
  await expect(page.getByText("Wake-Talk", { exact: true })).toBeVisible();
  await page.evaluate(() =>
    Reflect.get(window, "testVoiceEmit")("show-wake-talk", null),
  );
  await expect(
    page.getByRole("heading", { name: "Wake-Talk", exact: true }),
  ).toBeVisible();
});
