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
    const codexStatus = {
      settings: {
        feature_enabled: false,
        listening: false,
        phrase: "hey codex",
        shortcut: "Ctrl+Alt+Shift+Space",
        thread_id: "01a1a777-2733-73c3-999d-c7d0e0f7cfaf",
      },
      phase: "disabled",
      model: null,
      error: null,
      delivery: null,
      pending_text: null,
      pending_thread: null,
      history: [],
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
      testCodexCheckError: false,
      testCodexConfigurations: [],
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
          if (command === "clear_codex_message_history") {
            if (Reflect.get(window, "testCodexClearError"))
              throw "Delivery is still in progress";
            codexStatus.history = [];
            codexStatus.pending_text = null;
            codexStatus.pending_thread = null;
            codexStatus.delivery = null;
            codexStatus.error = null;
            emit("codex-talk-status", structuredClone(codexStatus));
            return;
          }
          if (command === "discard_codex_message") {
            codexStatus.pending_text = null;
            codexStatus.pending_thread = null;
            codexStatus.delivery = null;
            emit("codex-talk-status", structuredClone(codexStatus));
            return;
          }
          if (command === "get_codex_talk_status")
            return structuredClone(codexStatus);
          if (command === "load_codex_conversation") {
            if (Reflect.get(window, "testCodexCheckError"))
              throw "Thread is not accessible";
            return {
              thread_id: args.threadId,
              model: "thread-selected-model",
              active: false,
              messages: Reflect.get(
                window,
                "testCodexConversationMessages",
              ) ?? [
                {
                  role: "user",
                  text: "Earlier desktop message",
                  timestamp: 1700000000000,
                  turn_id: "old-turn",
                },
              ],
            };
          }
          if (command === "clear_codex_conversation") {
            Reflect.set(window, "testCodexConversationMessages", [
              {
                role: "assistant",
                text: "Ready",
                timestamp: 1700000000001,
                turn_id: "reset-turn",
              },
            ]);
            return {
              thread_id: args.threadId,
              model: "thread-selected-model",
              active: false,
              messages: Reflect.get(window, "testCodexConversationMessages"),
            };
          }
          if (command === "check_codex_thread") {
            if (Reflect.get(window, "testCodexCheckError"))
              throw "Thread is not accessible";
            return "thread-selected-model";
          }
          if (command === "configure_codex_talk") {
            Reflect.get(window, "testCodexConfigurations").push(args.config);
            Object.assign(codexStatus.settings, args.config);
            codexStatus.phase = codexStatus.settings.feature_enabled
              ? codexStatus.settings.listening
                ? "listening"
                : "paused"
              : "disabled";
            emit("codex-talk-status", structuredClone(codexStatus));
            return;
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

test("Codex-Talk validates its destination and keeps its triggers separate", async ({
  page,
}, testInfo) => {
  await page.goto("/");
  await page.getByText("Codex-Talk", { exact: true }).click();
  await expect(page.getByLabel("Thread ID")).toHaveValue(
    "01a1a777-2733-73c3-999d-c7d0e0f7cfaf",
  );
  await expect(
    page.getByText("Accessible · model: thread-selected-model"),
  ).toBeVisible();
  await page.getByLabel("Wake word or phrase").fill("computer codex");
  await page.getByLabel("Dedicated shortcut").fill("Ctrl+Alt+C");
  await page.getByRole("button", { name: "Save settings" }).click();
  await page.getByRole("switch", { name: "Enable Codex-Talk" }).check();
  await page
    .getByRole("button", { name: "Start listening", exact: true })
    .click();
  await expect(page.getByLabel("Wake word or phrase")).toBeDisabled();
  await page
    .getByRole("button", { name: "Pause listening", exact: true })
    .click();
  await expect(page.getByLabel("Wake word or phrase")).toBeEnabled();
  const data = await page.evaluate(() => ({
    codex: Reflect.get(window, "testCodexConfigurations"),
    voice: Reflect.get(window, "testVoiceConfigurations"),
  }));
  expect(data.codex.at(-1)).toMatchObject({
    feature_enabled: true,
    listening: false,
    phrase: "computer codex",
    shortcut: "Ctrl+Alt+C",
  });
  expect(data.voice).toHaveLength(0);
  await page.screenshot({ path: testInfo.outputPath("codex-talk.png") });
});

test("Codex-Talk blocks enabling when a thread cannot be accessed", async ({
  page,
}) => {
  await page.goto("/");
  await page.evaluate(() => Reflect.set(window, "testCodexCheckError", true));
  await page.getByText("Codex-Talk", { exact: true }).click();
  await expect(
    page.getByText("Cannot access thread: Thread is not accessible"),
  ).toBeVisible();
  await expect(
    page.getByRole("switch", { name: "Enable Codex-Talk" }),
  ).toBeDisabled();
  await expect(
    page.getByRole("button", { name: "Save settings" }),
  ).toBeDisabled();
});

test("failed Codex delivery retains the transcript and original destination", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByText("Codex-Talk", { exact: true }).click();
  await expect(page.getByLabel("Thread ID")).toBeVisible();
  await page.evaluate(() =>
    Reflect.get(window, "testVoiceEmit")("codex-talk-status", {
      settings: {
        feature_enabled: true,
        listening: false,
        phrase: "hey codex",
        shortcut: "Ctrl+Alt+Shift+Space",
        thread_id: "01a1a777-2733-73c3-999d-c7d0e0f7cfaf",
      },
      phase: "paused",
      model: "thread-selected-model",
      error: "Delivery timed out",
      delivery: "failed",
      pending_text: "Please check the build",
      pending_thread: "original-thread",
    }),
  );
  await expect(
    page.getByText("Please check the build", { exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("original-thread", { exact: true }),
  ).toBeVisible();
  await page.getByRole("button", { name: "Retry delivery" }).click();
  expect(
    await page.evaluate(() => Reflect.get(window, "testVoiceCalls")),
  ).toContain("retry_codex_message");
  await page.getByRole("button", { name: "Discard retained message" }).click();
  await expect(
    page.getByText("Please check the build", { exact: true }),
  ).toHaveCount(0);
});

test("clearing Codex-Talk history clears local messages without changing the destination", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByText("Codex-Talk", { exact: true }).click();
  await expect(page.getByLabel("Thread ID")).toBeVisible();
  await page.evaluate(() =>
    Reflect.get(window, "testVoiceEmit")("codex-talk-status", {
      settings: {
        feature_enabled: false,
        listening: false,
        phrase: "hey codex",
        shortcut: "Ctrl+Alt+Shift+Space",
        thread_id: "01a1a777-2733-73c3-999d-c7d0e0f7cfaf",
      },
      phase: "disabled",
      model: "thread-selected-model",
      error: "Old delivery error",
      delivery: "failed",
      pending_text: "Retained failed message",
      pending_thread: "original-thread",
      history: [
        {
          timestamp: 1700000000000,
          text: "Sent message",
          thread_id: "original-thread",
          delivery: "sent",
        },
      ],
    }),
  );
  await expect(
    page.getByRole("list", { name: "Handy delivery log" }),
  ).toContainText("Sent message");
  await page.getByRole("button", { name: "Clear delivery log" }).click();
  await expect(page.getByText("No messages in this session.")).toBeVisible();
  await expect(
    page.getByText("Retained failed message", { exact: true }),
  ).toHaveCount(0);
  await expect(page.getByRole("alert")).toHaveCount(0);
  await expect(page.getByLabel("Thread ID")).toHaveValue(
    "01a1a777-2733-73c3-999d-c7d0e0f7cfaf",
  );
  const calls = await page.evaluate(() =>
    Reflect.get(window, "testVoiceCalls"),
  );
  expect(calls).toContain("clear_codex_message_history");
  expect(calls).not.toContain("configure_codex_talk");
  expect(calls).not.toContain("retry_codex_message");
});

test("Codex-Talk cannot clear a message while it is being sent", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByText("Codex-Talk", { exact: true }).click();
  await expect(page.getByLabel("Thread ID")).toBeVisible();
  await page.evaluate(() =>
    Reflect.get(window, "testVoiceEmit")("codex-talk-status", {
      settings: {
        feature_enabled: true,
        listening: false,
        phrase: "hey codex",
        shortcut: "Ctrl+Alt+Shift+Space",
        thread_id: "01a1a777-2733-73c3-999d-c7d0e0f7cfaf",
      },
      phase: "processing",
      model: "thread-selected-model",
      error: null,
      delivery: "sending",
      pending_text: "In-flight message",
      pending_thread: "original-thread",
      history: [],
    }),
  );
  await expect(
    page.getByRole("button", { name: "Clear delivery log" }),
  ).toBeDisabled();
  await expect(
    page.getByText("In-flight message", { exact: true }),
  ).toBeVisible();
});

test("Codex conversation loads existing desktop messages and clears the selected thread", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByText("Codex-Talk", { exact: true }).click();
  await expect(
    page.getByRole("list", { name: "Codex conversation" }),
  ).toContainText("Earlier desktop message");
  await page.getByRole("button", { name: "Clear Codex conversation" }).click();
  await expect(
    page.getByText("Earlier desktop message", { exact: true }),
  ).toHaveCount(0);
  await expect(page.getByText("Ready", { exact: true })).toBeVisible();
  await expect(page.getByLabel("Thread ID")).toHaveValue(
    "01a1a777-2733-73c3-999d-c7d0e0f7cfaf",
  );
  const calls = await page.evaluate(() =>
    Reflect.get(window, "testVoiceCalls"),
  );
  expect(calls).toContain("load_codex_conversation");
  expect(calls).toContain("clear_codex_conversation");
  expect(calls).not.toContain("clear_codex_message_history");
});

test("Wake-Talk silence is entered in seconds with one decimal place", async ({
  page,
}) => {
  await page.goto("/");
  await page.getByText("Wake-Talk", { exact: true }).click();
  const timeout = page.getByRole("spinbutton", {
    name: "Finish after silence",
  });
  await expect(timeout).toHaveAttribute("step", "0.1");
  await timeout.fill("0.1");
  await page.getByRole("button", { name: "Save settings" }).click();
  const values = await page.evaluate(() =>
    Reflect.get(window, "testVoiceConfigurations"),
  );
  expect(values.at(-1)).toMatchObject({ silenceMs: 100 });
  await timeout.fill("0.12");
  expect(await timeout.inputValue()).not.toBe("0.12");
  await timeout.fill("1.5");
  await page.getByRole("button", { name: "Save settings" }).click();
  const updated = await page.evaluate(() =>
    Reflect.get(window, "testVoiceConfigurations"),
  );
  expect(updated.at(-1)).toMatchObject({ silenceMs: 1500 });
});
