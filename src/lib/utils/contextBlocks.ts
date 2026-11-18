export const PASTED_TEXT_BLOCK =
  /<pasted-text name="Text #(\d+)">\n([\s\S]*?)\n<\/pasted-text>(\n\n(?=<pasted-text name="Text #\d+">))?/g;

export interface ContextTextItem {
  number: number | null;
  text: string;
}

export function parseContextTextItems(context: string): ContextTextItem[] {
  const numbered = [...context.matchAll(PASTED_TEXT_BLOCK)].map((match) => ({
    number: Number(match[1]),
    text: match[2],
  }));
  const outside = context.replace(PASTED_TEXT_BLOCK, "").trim();
  return outside ? [...numbered, { number: null, text: outside }] : numbered;
}
