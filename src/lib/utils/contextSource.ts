const MARKER_LINE = /^---[ \t]*text[ \t]*$/i;
const DAMAGED_MARKER_CANDIDATE = /^---[ \t]*[a-z]{1,6}[ \t]*$/i;
const MARKER_TEXT = "--- text";

export type SourceLine =
  | { kind: "marker"; text: string; number: number | null }
  | { kind: "damaged"; text: string; itemNumber: number }
  | { kind: "content"; text: string };

export interface PastePlacement {
  start: number;
  end: number;
  text: string;
}

export interface PasteOptions {
  threshold: number;
  plain: boolean;
}

interface SourceItem {
  lines: string[];
  number: number | null;
}

const isMarkerLine = (line: string): boolean => MARKER_LINE.test(line);

const isDamagedMarkerLine = (line: string): boolean => DAMAGED_MARKER_CANDIDATE.test(line) && !isMarkerLine(line);

const isBlank = (text: string): boolean => text.trim() === "";

function splitItems(source: string): SourceItem[] {
  const items: SourceItem[] = [{ lines: [], number: null }];
  for (const line of source.split("\n")) {
    if (isMarkerLine(line)) {
      items.push({ lines: [], number: null });
    } else {
      items[items.length - 1].lines.push(line);
    }
  }
  let next = 1;
  for (const item of items) {
    if (!isBlank(item.lines.join("\n"))) {
      item.number = next++;
    }
  }
  return items;
}

export function parseContextSource(source: string): string[] {
  return splitItems(source)
    .filter((item) => item.number !== null)
    .map((item) => item.lines.join("\n"));
}

export function serializeContextSource(items: string[]): string {
  const kept = items.filter((text) => !isBlank(text));
  if (kept.length === 0) return "";
  if (kept.length === 1) return kept[0];
  return kept.map((text) => `${MARKER_TEXT}\n${text}`).join("\n");
}

export function classifyLines(source: string): SourceLine[] {
  const items = splitItems(source);
  const result: SourceLine[] = [];
  let itemIndex = 0;
  let markerSeen = false;
  for (const line of source.split("\n")) {
    if (isMarkerLine(line)) {
      itemIndex = markerSeen ? itemIndex + 1 : 1;
      markerSeen = true;
      result.push({ kind: "marker", text: line, number: items[itemIndex].number });
    } else if (isDamagedMarkerLine(line)) {
      result.push({ kind: "damaged", text: line, itemNumber: items[itemIndex].number ?? 0 });
    } else {
      result.push({ kind: "content", text: line });
    }
  }
  return result;
}

export function placePaste(
  source: string,
  selectionStart: number,
  selectionEnd: number,
  pasted: string,
  opts: PasteOptions,
): PastePlacement {
  if (opts.plain || selectionStart !== selectionEnd || pasted.length < opts.threshold) {
    return { start: selectionStart, end: selectionEnd, text: pasted };
  }
  const offset = itemPasteOffset(source, selectionStart);
  const firstPastedLine = pasted.split("\n", 1)[0];
  const body = isMarkerLine(firstPastedLine) ? pasted : `${MARKER_TEXT}\n${pasted}`;
  const leading = offset > 0 && source[offset - 1] !== "\n" ? "\n" : "";
  const trailing = offset < source.length && source[offset] !== "\n" && !body.endsWith("\n") ? "\n" : "";
  return { start: offset, end: offset, text: leading + body + trailing };
}

function itemPasteOffset(source: string, cursor: number): number {
  if (source === "") return cursor;
  const lineStarts: number[] = [];
  const lines = source.split("\n");
  let position = 0;
  for (const line of lines) {
    lineStarts.push(position);
    position += line.length + 1;
  }
  let cursorLine = lines.length - 1;
  while (lineStarts[cursorLine] > cursor) cursorLine--;

  let nextMarkerLine = cursorLine + 1;
  while (nextMarkerLine < lines.length && !isMarkerLine(lines[nextMarkerLine])) nextMarkerLine++;

  const restOfItemIsEmpty = lines.slice(cursorLine, nextMarkerLine).every((line) => line === "");
  if (restOfItemIsEmpty) return cursor;
  return nextMarkerLine < lines.length ? lineStarts[nextMarkerLine] : source.length;
}
