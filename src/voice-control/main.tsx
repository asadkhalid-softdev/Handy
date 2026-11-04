import React from "react";
import ReactDOM from "react-dom/client";
import { useTranslation } from "react-i18next";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { GripVertical, Pause, Play, Settings } from "lucide-react";
import { useVoiceControl } from "./useVoiceControl";
import "@/i18n";
import "./style.css";

function VoiceControl() {
  const { t } = useTranslation();
  const { status, pending, error, configure } = useVoiceControl();
  const phase = status?.phase ?? "starting";
  const enabled = status?.enabled ?? false;
  const featureEnabled = status?.feature_enabled ?? false;
  const [windowError, setWindowError] = React.useState(false);
  const reportWindowError = () => setWindowError(true);
  return (
    <main
      className={`voice-panel ${phase} ${error || windowError ? "has-error" : ""}`}
      aria-label={t("sidebar.wakeTalk")}
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
      <span className="sr-only" aria-live="polite" role="status">
        {t(`voiceControl.phase.${phase}`)}
      </span>
      <button
        className={`voice-toggle ${enabled ? "active" : ""}`}
        disabled={!status || !featureEnabled || pending}
        aria-label={t(enabled ? "voiceControl.pause" : "voiceControl.resume")}
        title={`${t(`voiceControl.phase.${phase}`)} — ${t(enabled ? "voiceControl.pause" : "voiceControl.resume")}`}
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
        aria-label={t("voiceControl.openWakeTalk")}
        title={t(
          error || windowError
            ? "voiceControl.checkSettings"
            : "voiceControl.openWakeTalk",
        )}
        onClick={() => void invoke("open_wake_talk").catch(reportWindowError)}
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
