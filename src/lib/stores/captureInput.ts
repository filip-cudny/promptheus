import type { ConversationImage } from "../types/conversation";

export type CapturedInput =
  | { kind: "image"; image: ConversationImage }
  | { kind: "text"; text: string };

export function addCaptured(
  tab: { input_images: ConversationImage[]; input_text_attachments: string[] } | undefined,
  captured: CapturedInput,
): boolean {
  if (!tab) return false;
  if (captured.kind === "image") {
    tab.input_images = [...tab.input_images, captured.image];
  } else {
    tab.input_text_attachments = [...tab.input_text_attachments, captured.text];
  }
  return true;
}
