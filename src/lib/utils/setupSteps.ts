import type { ModelConfig, ModelType, Surfaces } from "../types";
import type { SyncState } from "../services/settingsSync.ts";
import { agentModelId } from "./agentModel.ts";
import { parseEnvRef } from "./envVarOptions.ts";
import { envLink } from "./settingsCrossLinks.ts";

export const SETUP_STEP_COUNT = 5;

const NEW_TEXT_MODEL_KEY = "models:new";
const NEW_SPEECH_MODEL_KEY = "models:new-stt";

export interface SetupInputs {
  surfaces: Surfaces;
  models: ModelConfig[];
  isEnvSet: (name: string) => boolean;
  syncState: SyncState | null;
  textPassed: boolean;
  speechPassed: boolean;
  contextMenuShortcut: string | null;
}

export interface SetupStep {
  step: number;
  title: string;
  body: string;
  optional: boolean;
  done: boolean;
  navigate: string;
  highlight: string[];
  showSkip: boolean;
  showAgent: boolean;
  showTest: boolean;
  isLast: boolean;
  canNext: boolean;
}

export interface SetupTestResult {
  ok: boolean;
  millis: number | null;
  error: string | null;
}

export function clampStep(step: number): number {
  if (!Number.isInteger(step)) return 1;
  return Math.min(SETUP_STEP_COUNT, Math.max(1, step));
}

function effectiveTextModel(surfaces: Surfaces, models: ModelConfig[]): ModelConfig | null {
  const id = agentModelId(surfaces);
  return models.find((m) => m.id === id && m.type === "text") ?? null;
}

function speechModel(surfaces: Surfaces, models: ModelConfig[]): ModelConfig | null {
  const id = surfaces.speech_to_text.model_id;
  return models.find((m) => m.id === id && m.type === "stt") ?? null;
}

function missingKeyVariable(inputs: SetupInputs): string | null {
  const model = effectiveTextModel(inputs.surfaces, inputs.models);
  const name = parseEnvRef(model?.api_key);
  return name && !inputs.isEnvSet(name) ? name : null;
}

export function testableModel(
  step: number,
  surfaces: Surfaces,
  models: ModelConfig[],
): ModelConfig | null {
  if (step === 2) return effectiveTextModel(surfaces, models);
  if (step === 3) return speechModel(surfaces, models);
  return null;
}

export function createdModelPassed(
  type: ModelType,
  modelId: string,
  testPassed: boolean,
  surfaces: Surfaces,
): boolean {
  const assigned =
    type === "text" ? agentModelId(surfaces) : surfaces.speech_to_text.model_id;
  return testPassed && assigned === modelId;
}

export function testNote(result: SetupTestResult): string {
  if (result.ok) return `OK · ${result.millis ?? 0} ms`;
  return result.error || "Test failed";
}

interface StepDefinition {
  title: string;
  body: string;
  optional: boolean;
  done: boolean;
  navigate: string;
  highlight: string[];
}

function definition(step: number, inputs: SetupInputs): StepDefinition {
  switch (step) {
    case 1:
      return {
        title: "Bring your settings from a git repository",
        body: "Already use Promptheus on another machine? Connect the repository that holds its settings. Otherwise skip this step.",
        optional: true,
        done: inputs.syncState !== null && inputs.syncState !== "off",
        navigate: "section:sync",
        highlight: ["sync-connect"],
      };
    case 2: {
      const missing = missingKeyVariable(inputs);
      if (missing) {
        return {
          title: "Add the API key for your models",
          body: "Your repository brought the models; API keys are not synced. Paste the key on this machine.",
          optional: false,
          done: inputs.textPassed,
          navigate: envLink(missing),
          highlight: [`env-var-${missing}`],
        };
      }
      return {
        title: "Add a text model",
        body: "Everything starts with a text model: pick a provider, paste its API key and test it.",
        optional: false,
        done: inputs.textPassed,
        navigate: NEW_TEXT_MODEL_KEY,
        highlight: [],
      };
    }
    case 3:
      return {
        title: "Add a speech model",
        body: "Dictation turns your voice into text: pick a provider, paste its API key and test the model. Skip it if you do not dictate.",
        optional: true,
        done: inputs.speechPassed,
        navigate: NEW_SPEECH_MODEL_KEY,
        highlight: [],
      };
    case 4:
      return {
        title: "Choose your shortcuts",
        body: "These two open the context menu over selected text and start dictation. Add a shortcut to each, or keep the defaults.",
        optional: false,
        done: true,
        navigate: "section:shortcuts",
        highlight: ["keymaps.open_context_menu", "keymaps.speech_to_text_toggle"],
      };
    default:
      return {
        title: "You're set",
        body: `Select text in any app and press ${inputs.contextMenuShortcut ?? "your context menu shortcut"}.`,
        optional: false,
        done: true,
        navigate: "section:shortcuts",
        highlight: [],
      };
  }
}

export function setupStep(rawStep: number, inputs: SetupInputs): SetupStep {
  const step = clampStep(rawStep);
  const def = definition(step, inputs);
  return {
    step,
    ...def,
    showSkip: def.optional && !def.done,
    showAgent: step >= 3,
    showTest: step === 2 || step === 3,
    isLast: step === SETUP_STEP_COUNT,
    canNext: def.done,
  };
}
