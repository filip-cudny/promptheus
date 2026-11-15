const ENTRY_LINE = /^\s*(live|restart)\((.*)\),\s*$/;
const APPLIES = { live: "live", restart: "after_restart" };
const OPTION = /^(?:Some\((-?\d+)\)|None)$/;

function snakeCase(name) {
  return name.replace(/(?<=[a-z0-9])([A-Z])/g, "_$1").toLowerCase();
}

function splitTopLevel(text) {
  const parts = [];
  let depth = 0;
  let inString = false;
  let start = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (inString) {
      if (c === "\\") i++;
      else if (c === '"') inString = false;
    } else if (c === '"') inString = true;
    else if ("([{".includes(c)) depth++;
    else if (")]}".includes(c)) depth--;
    else if (c === "," && depth === 0) {
      parts.push(text.slice(start, i).trim());
      start = i + 1;
    }
  }
  const last = text.slice(start).trim();
  if (last) parts.push(last);
  return parts;
}

function parseString(token) {
  if (!/^"(?:[^"\\]|\\.)*"$/.test(token)) throw new Error(`expected a string literal, got ${token}`);
  return JSON.parse(token);
}

function parseOption(token) {
  const match = OPTION.exec(token);
  if (!match) throw new Error(`expected Some(<integer>) or None, got ${token}`);
  return match[1] === undefined ? null : Number(match[1]);
}

function parseFields(body) {
  return Object.fromEntries(
    splitTopLevel(body).map((field) => {
      const colon = field.indexOf(":");
      if (colon < 0) throw new Error(`expected <field>: <value>, got ${field}`);
      return [field.slice(0, colon).trim(), field.slice(colon + 1).trim()];
    }),
  );
}

const STRUCT_KINDS = {
  Integer: (f) => ({ type: "integer", min: parseOption(f.min), max: parseOption(f.max) }),
  Text: (f) => ({ type: "text", max_len: parseOption(f.max_len) }),
  Choice: (f) => {
    const list = /^&\[(.*)\]$/.exec(f.values ?? "");
    if (!list) throw new Error(`expected &[...] values, got ${f.values}`);
    return { type: "choice", values: splitTopLevel(list[1]).map(parseString) };
  },
  Model: (f) => {
    const model = /^ModelKind::(Text|Stt)$/.exec(f.model_type ?? "");
    if (!model) throw new Error(`unknown model type ${f.model_type}`);
    return { type: "model", model_type: snakeCase(model[1]) };
  },
};
const UNIT_KINDS = ["Bool", "List", "Map", "Shortcut", "Composite"];

function parseKind(token, constants) {
  if (token in constants) return constants[token];
  const int = /^int\((.*)\)$/.exec(token);
  if (int) {
    const [min, max, extra] = splitTopLevel(int[1]);
    if (max === undefined || extra !== undefined) throw new Error(`int expects 2 arguments, got ${token}`);
    return { type: "integer", min: parseOption(min), max: parseOption(max) };
  }
  const struct = /^Kind::(\w+)\s*\{(.*)\}$/.exec(token);
  if (struct && struct[1] in STRUCT_KINDS) return STRUCT_KINDS[struct[1]](parseFields(struct[2]));
  const unit = /^Kind::(\w+)$/.exec(token);
  if (unit && UNIT_KINDS.includes(unit[1])) return { type: snakeCase(unit[1]) };
  throw new Error(`unknown kind or constant ${token}`);
}

function parseConstants(lines) {
  const constants = {};
  for (const line of lines) {
    const match = /^const (\w+): Kind = (.*);$/.exec(line.trim());
    if (match) constants[match[1]] = parseKind(match[2], constants);
  }
  return constants;
}

function parseSections(source) {
  const body = /pub enum Section \{([^}]*)\}/.exec(source);
  if (!body) throw new Error("catalog.rs: missing `pub enum Section`");
  return new Set(splitTopLevel(body[1]).map((variant) => variant.replace(/\/\/.*$/gm, "").trim()));
}

function parseEntry(match, constants, sections) {
  const args = splitTopLevel(match[2]);
  if (args.length !== 5) throw new Error(`expected 5 arguments, got ${args.length}`);
  const [key, section, label, description, kind] = args;
  if (!sections.has(section)) throw new Error(`unknown section ${section}`);
  return {
    key: parseString(key),
    section: snakeCase(section),
    label: parseString(label),
    description: parseString(description),
    kind: parseKind(kind, constants),
    applies: APPLIES[match[1]],
  };
}

/** Rebuilds `get_settings_catalog`'s answer from the text of `catalog.rs`; throws naming the line it cannot parse. */
export function parseCatalog(source) {
  const lines = source.split("\n");
  const start = lines.findIndex((line) => /^pub const ENTRIES: &\[CatalogEntry\] = &\[\s*$/.test(line));
  if (start < 0) throw new Error("catalog.rs: missing `pub const ENTRIES: &[CatalogEntry] = &[`");
  const end = lines.findIndex((line, i) => i > start && line.trim() === "];");
  if (end < 0) throw new Error(`catalog.rs:${start + 1}: ENTRIES has no closing \`];\``);
  const constants = parseConstants(lines.slice(0, start));
  const sections = parseSections(source);
  return lines.slice(start + 1, end).map((line, i) => {
    const lineNumber = start + 2 + i;
    const match = ENTRY_LINE.exec(line);
    try {
      if (!match) throw new Error("not a live(...) or restart(...) entry");
      return parseEntry(match, constants, sections);
    } catch (e) {
      throw new Error(`catalog.rs:${lineNumber}: ${e.message}: ${line.trim()}`);
    }
  });
}
