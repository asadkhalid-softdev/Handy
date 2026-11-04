import React from "react";
import ReactDOM from "react-dom/client";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { GripVertical, Mic, MicOff, Pause, Play, Settings } from "lucide-react";
import { useVoiceControl } from "./useVoiceControl";
import "@/i18n";
import "./style.css";

function VoiceControl() {
  const { t } = useTranslation();
  const { status, pending, error, configure } = useVoiceControl();
  const phase = status?.phase ?? "starting";
  const enabled = status?.enabled ?? false;
  const [windowError, setWindowError] = React.useState(false);
  const reportWindowError = () => setWindowError(true);
  return (
    <main
      className={`voice-panel ${phase}`}
      aria-label={t("voiceControl.title")}
    >
      <button
        className="voice-grip"
        aria-label={t("voiceControl.move")}
        title={t("voiceControl.move")}
        onMouseDown={(event) => {
          if (event.button === 0) {
            event.preventDefault();
            void getCurrentWindow().startDragging().catch(reportWindowError);
          }
        }}
      >
        <GripVertical size={16} aria-hidden="true" />
      </button>
      <span className="voice-mic" aria-hidden="true">
        {enabled ? <Mic size={19} /> : <MicOff size={19} />}
      </span>
      <div className="voice-copy" aria-live="polite" role="status">
        <strong>{t(`voiceControl.phase.${phase}`)}</strong>
        <span title={error}>
          {error || windowError
            ? t("voiceControl.checkSettings")
            : phase === "listening"
              ? t("voiceControl.sayPhrase", { phrase: status?.phrase })
              : enabled
                ? t("voiceControl.waitForPink")
                : t("voiceControl.micReleased")}
        </span>
      </div>
      <button
        className={`voice-toggle ${enabled ? "active" : ""}`}
        disabled={!status || pending}
        aria-label={t(enabled ? "voiceControl.pause" : "voiceControl.resume")}
        title={t(enabled ? "voiceControl.pause" : "voiceControl.resume")}
        aria-pressed={enabled}
        onClick={() => void configure(!enabled)}
      >
        {enabled ? (
          <Pause size={19} aria-hidden="true" />
        ) : (
          <Play size={19} aria-hidden="true" />
        )}
      </button>
      <button
        className="voice-settings"
        aria-label={t("voiceControl.openSettings")}
        title={t("voiceControl.openSettings")}
        onClick={() =>
          void invoke("show_main_window_command").catch(reportWindowError)
        }
      >
        <Settings size={17} aria-hidden="true" />
      </button>
    </main>
  );
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <VoiceControl />
  </React.StrictMode>,
);
