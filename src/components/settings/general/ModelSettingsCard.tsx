import React, { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import { SettingsGroup } from "../../ui/SettingsGroup";
import { LanguageSelector } from "../LanguageSelector";
import { TranslateToEnglish } from "../TranslateToEnglish";
import { useModelStore } from "../../../stores/modelStore";
import {
  commands,
  type ModelInfo,
  type StreamingLatencyChoices,
} from "@/bindings";
import { useSettings } from "../../../hooks/useSettings";
import { Dropdown } from "../../ui/Dropdown";
import { SettingContainer } from "../../ui/SettingContainer";

export const ModelSettingsCard: React.FC = () => {
  const { t } = useTranslation();
  const { currentModel, models } = useModelStore();

  const { settings, updateSetting, isUpdating } = useSettings();
  const [latencyChoices, setLatencyChoices] = useState<{
    modelId: string;
    choices: StreamingLatencyChoices | null;
  } | null>(null);
  useEffect(() => {
    let active = true;
    if (currentModel) {
      void commands
        .getStreamingLatencyChoices(currentModel)
        .then((choices) => {
          if (active) setLatencyChoices({ modelId: currentModel, choices });
        })
        .catch(console.error);
    }
    return () => {
      active = false;
    };
  }, [currentModel]);
  const choices =
    latencyChoices?.modelId === currentModel ? latencyChoices.choices : null;
  const currentModelInfo = models.find((m: ModelInfo) => m.id === currentModel);

  const showLanguageSelector =
    currentModelInfo?.supports_language_selection ?? false;
  const supportsTranslation = currentModelInfo?.supports_translation ?? false;
  const hasAnySettings =
    showLanguageSelector || supportsTranslation || choices !== null;

  // Don't render anything if no model is selected or no settings available
  if (!currentModel || !currentModelInfo || !hasAnySettings) {
    return null;
  }

  return (
    <SettingsGroup
      title={t("settings.modelSettings.title", {
        model: currentModelInfo.name,
      })}
    >
      {choices && (
        <SettingContainer
          title={t("settings.modelSettings.streamingLatency.title")}
          description={t("settings.modelSettings.streamingLatency.description")}
          descriptionMode="inline"
          grouped={true}
        >
          <Dropdown
            selectedValue={String(
              settings?.streaming_latency_ms?.[currentModel] ?? "default",
            )}
            disabled={!settings || isUpdating("streaming_latency_ms")}
            options={[
              {
                value: "default",
                label: t("settings.modelSettings.streamingLatency.default", {
                  ms: choices.default_ms,
                }),
              },
              ...choices.presets.map((preset) => ({
                value: String(preset.latency_ms),
                label: t(
                  preset.accuracy_warning
                    ? "settings.modelSettings.streamingLatency.warningOption"
                    : "settings.modelSettings.streamingLatency.option",
                  { ms: preset.latency_ms },
                ),
              })),
            ]}
            onSelect={(value) => {
              const values = { ...settings?.streaming_latency_ms };
              if (value === "default") delete values[currentModel];
              else values[currentModel] = Number(value);
              void updateSetting("streaming_latency_ms", values);
            }}
          />
        </SettingContainer>
      )}
      {showLanguageSelector && (
        <LanguageSelector
          descriptionMode="tooltip"
          grouped={true}
          modelId={currentModelInfo.id}
          supportedLanguages={currentModelInfo.supported_languages}
        />
      )}
      {supportsTranslation && (
        <TranslateToEnglish descriptionMode="tooltip" grouped={true} />
      )}
    </SettingsGroup>
  );
};
