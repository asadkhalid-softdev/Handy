import { useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useTranslation } from "react-i18next";
import { Pause, Play } from "lucide-react";
import { SettingsGroup } from "../ui/SettingsGroup";

interface Config {
  feature_enabled: boolean;
  listening: boolean;
  phrase: string;
  shortcut: string;
  thread_id: string;
}
interface Status {
  settings: Config;
  phase: string;
  model: string | null;
  error: string | null;
  delivery: string | null;
  pending_text: string | null;
  pending_thread: string | null;
}
export function CodexTalkSettings() {
  const { t } = useTranslation();
  const [status, setStatus] = useState<Status | null>(null);
  const [draft, setDraft] = useState<Config | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [checkRevision, setCheckRevision] = useState(0);
  const [check, setCheck] = useState({ phase: "checking", model: "" });
  useEffect(() => {
    let active = true;
    const subscription = listen<Status>("codex-talk-status", ({ payload }) => {
      if (active) setStatus(payload);
    });
    void invoke<Status>("get_codex_talk_status")
      .then((value) => {
        if (active) {
          setStatus(value);
          setDraft(value.settings);
        }
      })
      .catch((e: unknown) => {
        if (active) setError(String(e));
      });
    return () => {
      active = false;
      void subscription.then((unlisten) => unlisten());
    };
  }, []);
  const threadId = draft?.thread_id.trim();
  useEffect(() => {
    if (!threadId) return;
    let active = true;
    setCheck({ phase: "checking", model: "" });
    const timer = setTimeout(() => {
      void invoke<string>("check_codex_thread", { threadId })
        .then((model) => {
          if (active) setCheck({ phase: "accessible", model });
        })
        .catch((e: unknown) => {
          if (active) setCheck({ phase: "failed", model: String(e) });
        });
    }, 600);
    return () => {
      active = false;
      clearTimeout(timer);
    };
  }, [threadId, checkRevision]);
  async function save(config: Config) {
    setBusy(true);
    setError(null);
    try {
      await invoke("configure_codex_talk", { config });
      setDraft(config);
    } catch (e) {
      setError(String(e));
    } finally {
      setBusy(false);
    }
  }
  const activeCapture = ["arming", "recording", "processing"].includes(
    status?.phase ?? "",
  );
  const editLocked =
    busy || activeCapture || (status?.settings.listening ?? false);
  return (
    <section
      className="max-w-3xl w-full mx-auto space-y-6"
      aria-label={t("sidebar.codexTalk")}
    >
      <header className="px-4 space-y-2">
        <h1 className="text-xl font-semibold">{t("sidebar.codexTalk")}</h1>
        <p className="text-sm text-text/70">{t("codexTalk.description")}</p>
      </header>
      {draft && (
        <>
          <SettingsGroup title={t("codexTalk.destination")}>
            <div className="space-y-4 p-4">
              <label className="block space-y-2">
                <span>{t("codexTalk.threadId")}</span>
                <input
                  className="border border-line rounded px-3 py-2 bg-transparent w-full font-mono text-sm"
                  value={draft.thread_id}
                  disabled={editLocked}
                  maxLength={36}
                  onChange={(event) =>
                    setDraft({ ...draft, thread_id: event.target.value })
                  }
                />
              </label>
              <p role="status" className="text-sm text-text/70">
                {t(`codexTalk.check.${check.phase}`)} {check.model}
              </p>
              <button
                className="text-sm underline disabled:opacity-40"
                disabled={check.phase === "checking"}
                onClick={() => setCheckRevision((value) => value + 1)}
              >
                {t("codexTalk.recheck")}
              </button>
              <p className="text-xs text-text/60">{t("codexTalk.modelHint")}</p>
            </div>
          </SettingsGroup>
          <SettingsGroup title={t("voiceControl.configuration")}>
            <div className="space-y-4 p-4">
              <label className="flex items-center justify-between gap-3">
                <span>{t("voiceControl.wakePhrase")}</span>
                <input
                  className="border border-line rounded px-3 py-2 bg-transparent w-52"
                  value={draft.phrase}
                  maxLength={60}
                  disabled={editLocked}
                  onChange={(e) =>
                    setDraft({ ...draft, phrase: e.target.value })
                  }
                />
              </label>
              <label className="flex items-center justify-between gap-3">
                <span>{t("codexTalk.shortcut")}</span>
                <input
                  className="border border-line rounded px-3 py-2 bg-transparent w-52"
                  value={draft.shortcut}
                  maxLength={60}
                  disabled={editLocked}
                  onChange={(e) =>
                    setDraft({ ...draft, shortcut: e.target.value })
                  }
                />
              </label>
              <p className="text-xs text-text/60">
                {t("codexTalk.triggerHint")}
              </p>
              <button
                className="border border-line rounded-lg px-4 py-2 text-sm disabled:opacity-40"
                disabled={editLocked || check.phase !== "accessible"}
                onClick={() => void save(draft)}
              >
                {t("voiceControl.save")}
              </button>
            </div>
          </SettingsGroup>
          <SettingsGroup title={t("voiceControl.feature")}>
            <div className="space-y-4 p-4">
              <label className="flex items-center justify-between gap-3 font-medium">
                <span>{t("codexTalk.enable")}</span>
                <input
                  type="checkbox"
                  role="switch"
                  className="h-4 w-4 accent-logo-primary"
                  checked={status?.settings.feature_enabled ?? false}
                  disabled={
                    busy ||
                    (!status?.settings.feature_enabled &&
                      check.phase !== "accessible")
                  }
                  onChange={(e) =>
                    void save({
                      ...(e.target.checked ? draft : status!.settings),
                      feature_enabled: e.target.checked,
                      listening: false,
                    })
                  }
                />
              </label>
              <div className="flex items-center justify-between gap-4">
                <p role="status" className="text-sm text-text/70">
                  {t(`codexTalk.phase.${status?.phase ?? "disabled"}`)}
                </p>
                <button
                  className="flex items-center gap-2 border border-line rounded-lg px-4 py-2 text-sm disabled:opacity-40"
                  disabled={busy || !status?.settings.feature_enabled}
                  onClick={() =>
                    void save({
                      ...draft,
                      listening: !status?.settings.listening,
                    })
                  }
                >
                  {status?.settings.listening ? (
                    <Pause size={16} aria-hidden="true" />
                  ) : (
                    <Play size={16} aria-hidden="true" />
                  )}
                  {t(
                    status?.settings.listening
                      ? "voiceControl.pauseAction"
                      : "voiceControl.playAction",
                  )}
                </button>
              </div>
              <p className="text-xs text-text/60">
                {t("codexTalk.listeningHint")}
              </p>
            </div>
          </SettingsGroup>
        </>
      )}
      {status?.delivery && (
        <p role="status" className="px-4 text-sm">
          {t(`codexTalk.delivery.${status.delivery}`)}
        </p>
      )}
      {status?.pending_text && (
        <div className="px-4 space-y-2">
          <p className="text-xs text-text/60">{t("codexTalk.retained")}</p>
          <pre className="whitespace-pre-wrap text-sm border border-line rounded p-3">
            {status.pending_text}
          </pre>
          <p className="text-xs font-mono">{status.pending_thread}</p>
          {status.delivery === "failed" && (
            <button
              className="border border-line rounded-lg px-4 py-2 text-sm"
              disabled={busy || !status.settings.feature_enabled}
              onClick={() => {
                setBusy(true);
                void invoke("retry_codex_message")
                  .catch((e: unknown) => setError(String(e)))
                  .finally(() => setBusy(false));
              }}
            >
              {t("codexTalk.retry")}
            </button>
          )}
          {status.delivery === "failed" && (
            <button
              className="ml-3 text-sm underline"
              disabled={busy}
              onClick={() =>
                void invoke("discard_codex_message").catch((e: unknown) =>
                  setError(String(e)),
                )
              }
            >
              {t("codexTalk.discard")}
            </button>
          )}
        </div>
      )}
      {(error || status?.error) && (
        <p role="alert" className="px-4 text-sm text-red-500">
          {error ?? status?.error}
        </p>
      )}
    </section>
  );
}
