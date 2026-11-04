import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { useVoiceControl } from "@/voice-control/useVoiceControl";
import { SettingsGroup } from "../ui/SettingsGroup";

export function VoiceControlSettings() {
  const { t } = useTranslation();
  const { status, pending, error, configure } = useVoiceControl();
  const [phrase, setPhrase] = useState("");
  const [silenceMs, setSilenceMs] = useState(2000);
  const [showError, setShowError] = useState(false);
  useEffect(() => {
    if (status) {
      setPhrase(status.phrase);
      setSilenceMs(status.silence_ms);
    }
  }, [status?.phrase, status?.silence_ms]);
  return (
    <SettingsGroup title={t("voiceControl.title")}>
      <div className="space-y-4 p-4">
        <p className="text-sm text-text/70">{t("voiceControl.description")}</p>
        <p className="text-sm text-text/70">{t("voiceControl.instructions")}</p>
        <label className="flex items-center justify-between gap-3">
          <span>{t("voiceControl.enable")}</span>
          <input
            type="checkbox"
            checked={status?.enabled ?? false}
            disabled={!status || pending}
            onChange={(event) => void configure(event.target.checked)}
          />
        </label>
        <label className="flex items-center justify-between gap-3">
          <span>{t("voiceControl.wakePhrase")}</span>
          <input
            className="border rounded px-2 py-1 bg-transparent w-48"
            value={phrase}
            maxLength={60}
            disabled={!status || status.enabled}
            onChange={(event) => setPhrase(event.target.value)}
          />
        </label>
        <label className="flex items-center justify-between gap-3">
          <span>{t("voiceControl.silenceTimeout")}</span>
          <select
            className="border rounded px-2 py-1 bg-background"
            value={silenceMs}
            disabled={!status || status.enabled}
            onChange={(event) => setSilenceMs(Number(event.target.value))}
          >
            {[1000, 1500, 2000, 3000, 5000].map((ms) => (
              <option key={ms} value={ms}>
                {t("voiceControl.seconds", { count: ms / 1000 })}
              </option>
            ))}
          </select>
        </label>
        <p className="text-xs text-text/60">{t("voiceControl.pauseToEdit")}</p>
        <div className="flex gap-3">
          <button
            className="border rounded px-3 py-1"
            disabled={!status || status.enabled || pending}
            onClick={() => void configure(false, phrase, silenceMs)}
          >
            {t("voiceControl.save")}
          </button>
          <button
            className="border rounded px-3 py-1"
            onClick={() =>
              void invoke("show_voice_control").catch(() => setShowError(true))
            }
          >
            {t("voiceControl.showPanel")}
          </button>
        </div>
        {(error || showError) && (
          <p role="alert" className="text-sm text-red-500">
            {error ?? t("voiceControl.checkSettings")}
          </p>
        )}
      </div>
    </SettingsGroup>
  );
}
