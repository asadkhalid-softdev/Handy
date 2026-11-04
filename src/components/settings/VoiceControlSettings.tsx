import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { Pause, Play, PanelsTopLeft } from "lucide-react";
import { useVoiceControl } from "@/voice-control/useVoiceControl";
import { SettingsGroup } from "../ui/SettingsGroup";

export function VoiceControlSettings() {
  const { t } = useTranslation();
  const { status, pending, error, configure } = useVoiceControl();
  const [phrase, setPhrase] = useState("");
  const [silenceMs, setSilenceMs] = useState(2000);
  const [showError, setShowError] = useState(false);
  const featureEnabled = status?.feature_enabled ?? false;
  const listening = status?.enabled ?? false;
  useEffect(() => {
    if (status) {
      setPhrase(status.phrase);
      setSilenceMs(status.silence_ms);
    }
  }, [status?.phrase, status?.silence_ms]);
  return (
    <section
      className="max-w-3xl w-full mx-auto space-y-6"
      aria-label={t("sidebar.wakeTalk")}
    >
      <header className="px-4 space-y-2">
        <h1 className="text-xl font-semibold">{t("sidebar.wakeTalk")}</h1>
        <p className="text-sm text-text/70">{t("voiceControl.description")}</p>
      </header>
      <SettingsGroup title={t("voiceControl.feature")}>
        <div className="space-y-2 p-4">
          <label className="flex items-center justify-between gap-3 font-medium">
            <span>{t("voiceControl.enableFeature")}</span>
            <input
              type="checkbox"
              role="switch"
              className="h-4 w-4 accent-logo-primary"
              checked={featureEnabled}
              disabled={!status || pending}
              onChange={(event) =>
                void configure(
                  false,
                  undefined,
                  undefined,
                  event.target.checked,
                )
              }
            />
          </label>
          <p className="text-xs text-text/60">
            {t("voiceControl.featureDescription")}
          </p>
        </div>
        <div className="flex items-center justify-between gap-4 p-4">
          <div className="space-y-1">
            <p className="text-sm font-medium">
              {t("voiceControl.microphone")}
            </p>
            <p className="text-sm text-text/70" role="status">
              {t(`voiceControl.phase.${status?.phase ?? "starting"}`)}
            </p>
          </div>
          <button
            className="flex items-center gap-2 border border-line rounded-lg px-4 py-2 text-sm disabled:opacity-40"
            disabled={!status || !featureEnabled || pending}
            aria-label={t(
              listening ? "voiceControl.pause" : "voiceControl.resume",
            )}
            aria-pressed={listening}
            onClick={() => void configure(!listening)}
          >
            {listening ? (
              <Pause size={16} aria-hidden="true" />
            ) : (
              <Play size={16} aria-hidden="true" />
            )}
            {t(
              listening
                ? "voiceControl.pauseAction"
                : "voiceControl.playAction",
            )}
          </button>
        </div>
      </SettingsGroup>
      <SettingsGroup title={t("voiceControl.configuration")}>
        <div className="space-y-4 p-4">
          <label className="flex items-center justify-between gap-3">
            <span>{t("voiceControl.wakePhrase")}</span>
            <input
              className="border border-line rounded px-3 py-2 bg-transparent w-52"
              value={phrase}
              maxLength={60}
              disabled={!status || listening}
              onChange={(event) => setPhrase(event.target.value)}
            />
          </label>
          <label className="flex items-center justify-between gap-3">
            <span>{t("voiceControl.silenceTimeout")}</span>
            <select
              className="border border-line rounded px-3 py-2 bg-background"
              value={silenceMs}
              disabled={!status || listening}
              onChange={(event) => setSilenceMs(Number(event.target.value))}
            >
              {[1000, 1500, 2000, 3000, 5000].map((ms) => (
                <option key={ms} value={ms}>
                  {t("voiceControl.seconds", { count: ms / 1000 })}
                </option>
              ))}
            </select>
          </label>
          <p className="text-xs text-text/60">
            {t("voiceControl.pauseToEdit")}
          </p>
          <button
            className="border border-line rounded-lg px-4 py-2 text-sm disabled:opacity-40"
            disabled={!status || listening || pending}
            onClick={() => void configure(false, phrase, silenceMs)}
          >
            {t("voiceControl.save")}
          </button>
        </div>
      </SettingsGroup>
      <div className="flex items-start justify-between gap-4 px-4">
        <p className="text-sm text-text/70 max-w-md">
          {t("voiceControl.instructions")}
        </p>
        <button
          className="flex shrink-0 items-center gap-2 border border-line rounded-lg px-3 py-2 text-sm disabled:opacity-40"
          disabled={!featureEnabled}
          onClick={() =>
            void invoke("show_voice_control").catch(() => setShowError(true))
          }
        >
          <PanelsTopLeft size={16} aria-hidden="true" />
          {t("voiceControl.showPanel")}
        </button>
      </div>
      {(error || showError) && (
        <p role="alert" className="px-4 text-sm text-red-500">
          {error ?? t("voiceControl.checkSettings")}
        </p>
      )}
    </section>
  );
}
