import { useCallback, useEffect, useState } from "react";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";

export interface VoiceStatus {
  feature_enabled: boolean;
  enabled: boolean;
  phase:
    | "paused"
    | "disabled"
    | "starting"
    | "listening"
    | "arming"
    | "recording"
    | "processing"
    | "error";
  phrase: string;
  silence_ms: number;
}

export function useVoiceControl() {
  const [status, setStatus] = useState<VoiceStatus>();
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string>();
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void (async () => {
      const stop = await listen<VoiceStatus>(
        "voice-control-status",
        (event) => {
          if (!disposed) setStatus(event.payload);
        },
      );
      if (disposed) {
        stop();
        return;
      }
      unlisten = stop;
      const current = await invoke<VoiceStatus>("get_voice_control_status");
      if (!disposed) setStatus(current);
    })().catch((cause) => {
      if (!disposed) setError(String(cause));
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, []);

  const configure = useCallback(
    async (
      enabled: boolean,
      phrase?: string,
      silenceMs?: number,
      featureEnabled?: boolean,
    ) => {
      if (!status || pending) return;
      setPending(true);
      setError(undefined);
      try {
        await invoke("configure_voice_control", {
          featureEnabled: featureEnabled ?? status.feature_enabled,
          enabled,
          phrase: phrase ?? status.phrase,
          silenceMs: silenceMs ?? status.silence_ms,
        });
      } catch (cause) {
        setError(String(cause));
      } finally {
        setPending(false);
      }
    },
    [status, pending],
  );

  return { status, pending, error, configure };
}
