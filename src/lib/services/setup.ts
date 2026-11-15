import { invoke } from "@tauri-apps/api/core";
import type { SurfaceKind } from "$lib/types";

export interface SetupStatus {
  active: boolean;
  step: number;
}

export async function getSetupStatus(): Promise<SetupStatus> {
  return invoke("get_setup_status");
}

export async function finishSetup(): Promise<void> {
  return invoke("finish_setup");
}

export async function assignSetupModel(
  modelId: string,
): Promise<SurfaceKind | null> {
  return invoke("assign_setup_model", { modelId });
}
