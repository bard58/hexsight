// Review every text of the interface in one file.
//
//   node tools/texts.mjs export <out.tsv> [--md <out.md>] [--src <i18n.rs>]
//   node tools/texts.mjs import <in.tsv> [--src <i18n.rs>] [--dry-run]
//
// export: reads the table in src/i18n.rs and writes one row for each key:
//   Key, Screen, Türkçe, English, Used in, Note.
// "Used in" lists the places (file:line) that name Key::<Name> in src/*.rs,
// or says "unused". The TSV is UTF-8 with a BOM, so Excel shows Turkish letters.
// With --md it also writes a table for reading, grouped by screen.
//
// import: reads the edited file and writes the changed Türkçe and English
// texts back into i18n.rs. Only the text between the quotes of a changed
// text is replaced. Every other byte of the file stays as it is. Nothing is
// written when a check fails (unknown key, empty text, {0} places that
// differ between the two languages).
//
// The file can come back from Excel as tab-delimited text, as "Unicode Text"
// (UTF-16) or as CSV with ; or , between the cells. Quoted cells work.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const SRC_DIR = path.join(root, "src");

// Screens in the order of the review file. The key is the heading of a
// section in the table of i18n.rs (the text after "// ---- ").
const SCREENS = [
  "Title bar",
  "Champion select",
  "In game",
  "Pick card",
  "Overlay",
  "After the game",
  "My games",
  "Settings",
  "About",
  "Messages and errors",
];

// The screen of each section of the table, by the start of its heading.
const SECTION_SCREEN = [
  ["Title bar", "Title bar"],
  ["Main page", "Champion select"],
  ["First-run checklist", "Champion select"],
  ["Footer", "Champion select"],
  ["Waiting in a game", "In game"],
  ["Pick screen", "Pick card"],
  ["After the game", "After the game"],
  ["Bench", "Champion select"],
  ["My games", "My games"],
  ["Settings: about", "About"],
  ["Settings", "Settings"],
  ["Overlay", "Overlay"],
  ["Messages from the worker", "Messages and errors"],
  ["Errors", "Messages and errors"],
  ["Item set", "In game"],
  ["Build styles", "Champion select"],
];

// Keys that belong to another screen than their section says.
const SCREEN_OF_KEY = {
  HotkeyCode: "Settings", // the name of a hotkey
  GroupAbout: "About", // the heading of the About group
  OcrUnavailable: "Messages and errors",
};

function screenOf(key, section) {
  if (SCREEN_OF_KEY[key]) return SCREEN_OF_KEY[key];
  const hit = SECTION_SCREEN.find(([start]) => section.startsWith(start));
  return hit ? hit[1] : section || "Other";
}

// ---- Reading i18n.rs ---------------------------------------------------------

// Reads one Rust string literal that starts at `start` (the opening quote).
// Returns the text and the index after the closing quote.
function readLiteral(src, start) {
  let i = start + 1;
  let text = "";
  for (;;) {
    if (i >= src.length) throw new Error(`string without end at offset ${start}`);
    const c = src[i];
    if (c === '"') return { text, end: i + 1 };
    if (c !== "\\") {
      text += c;
      i += 1;
      continue;
    }
    const e = src[i + 1];
    i += 2;
    switch (e) {
      case "n": text += "\n"; break;
      case "r": text += "\r"; break;
      case "t": text += "\t"; break;
      case "0": text += "\0"; break;
      case "\\": text += "\\"; break;
      case '"': text += '"'; break;
      case "'": text += "'"; break;
      case "x": {
        text += String.fromCharCode(parseInt(src.slice(i, i + 2), 16));
        i += 2;
        break;
      }
      case "u": {
        const close = src.indexOf("}", i);
        text += String.fromCodePoint(parseInt(src.slice(i + 1, close).replace(/_/g, ""), 16));
        i = close + 1;
        break;
      }
      case "\n":
      case "\r": {
        // A backslash at the end of a line drops the line break and the
        // spaces that start the next line.
        while (i < src.length && /\s/.test(src[i])) i += 1;
        break;
      }
      default:
        throw new Error(`unknown escape \\${e} at offset ${i - 2}`);
    }
  }
}

// Writes a text as the inside of a Rust string literal, on one line.
function encodeLiteral(text) {
  let out = "";
  for (const ch of text) {
    const cp = ch.codePointAt(0);
    if (ch === "\\") out += "\\\\";
    else if (ch === '"') out += '\\"';
    else if (ch === "\n") out += "\\n";
    else if (ch === "\r") out += "\\r";
    else if (ch === "\t") out += "\\t";
    else if (cp < 0x20 || cp === 0x7f) out += `\\u{${cp.toString(16)}}`;
    else out += ch;
  }
  return `"${out}"`;
}

// Parses the table! { ... } block. Returns the entries in table order:
// { key, section, note, en, tr } where en and tr are { text, start, end }
// (the span of the literal in the source, quotes included).
function parseTable(src) {
  const open = src.indexOf("table! {");
  if (open < 0) throw new Error("table! { not found");
  let i = src.indexOf("\n", open) + 1;
  const entries = [];
  let section = "";
  let notes = [];
  const skipSpace = () => {
    while (i < src.length && /\s/.test(src[i])) i += 1;
  };
  const expect = (ch) => {
    skipSpace();
    if (src[i] !== ch) {
      throw new Error(`expected "${ch}" at line ${lineOf(src, i)}, found "${src.slice(i, i + 20)}"`);
    }
    i += 1;
  };
  const literal = () => {
    skipSpace();
    if (src[i] !== '"') throw new Error(`expected a string at line ${lineOf(src, i)}`);
    const { text, end } = readLiteral(src, i);
    const span = { text, start: i, end };
    i = end;
    return span;
  };
  for (;;) {
    skipSpace();
    if (src[i] === "}") break;
    if (src.startsWith("//", i)) {
      let eol = src.indexOf("\n", i);
      if (eol < 0) eol = src.length;
      const line = src.slice(i + 2, eol).trim();
      if (line.startsWith("----")) {
        section = line.replace(/^-+\s*/, "");
        notes = [];
      } else {
        notes.push(line);
      }
      i = eol;
      continue;
    }
    const m = /^[A-Za-z_][A-Za-z0-9_]*/.exec(src.slice(i, i + 80));
    if (!m) throw new Error(`expected a key at line ${lineOf(src, i)}`);
    const key = m[0];
    i += key.length;
    expect("=");
    const en = literal();
    expect(",");
    const tr = literal();
    expect(";");
    entries.push({ key, section, note: notes.join(" "), en, tr });
    notes = [];
  }
  return entries;
}

function lineOf(src, offset) {
  let n = 1;
  for (let k = 0; k < offset; k += 1) if (src[k] === "\n") n += 1;
  return n;
}

// ---- Where a key is used --------------------------------------------------------

// Lines of a source file that count as code of the program: everything but the
// tests module at the end. Returns a function line number -> is test.
function testStart(lines) {
  for (let n = 0; n < lines.length - 1; n += 1) {
    if (lines[n] === "#[cfg(test)]" && /^mod\s/.test(lines[n + 1])) return n + 1;
  }
  return Infinity;
}

// Returns Map key -> { code: ["app.rs:12", ...], tests: [...] }.
function findUses(keys, tableLines) {
  const uses = new Map(keys.map((k) => [k, { code: [], tests: [] }]));
  const names = new Set(keys);
  for (const file of fs.readdirSync(SRC_DIR).filter((f) => f.endsWith(".rs")).sort()) {
    const lines = fs.readFileSync(path.join(SRC_DIR, file), "utf8").split("\n");
    const firstTest = testStart(lines);
    lines.forEach((line, n) => {
      const num = n + 1;
      // The table itself and its comments do not count as a use.
      if (file === "i18n.rs" && num >= tableLines[0] && num <= tableLines[1]) return;
      // Skip text in a comment.
      const code = line.replace(/\/\/.*$/, "");
      for (const m of code.matchAll(/\bKey::([A-Za-z_][A-Za-z0-9_]*)/g)) {
        if (!names.has(m[1])) continue;
        const bucket = n + 1 >= firstTest ? uses.get(m[1]).tests : uses.get(m[1]).code;
        const where = `${file}:${num}`;
        if (!bucket.includes(where)) bucket.push(where);
      }
    });
  }
  return uses;
}

// ---- Cells ---------------------------------------------------------------------

function quoteCell(value) {
  return /[\t"\r\n]/.test(value) ? `"${value.replace(/"/g, '""')}"` : value;
}

// Splits a delimited text into rows of cells. Cells in quotes may hold the
// delimiter, line breaks and "" for a quote.
function parseCells(text, delim) {
  const rows = [];
  let row = [];
  let cell = "";
  let quoted = false;
  let atStart = true;
  for (let i = 0; i < text.length; i += 1) {
    const c = text[i];
    if (quoted) {
      if (c === '"') {
        if (text[i + 1] === '"') {
          cell += '"';
          i += 1;
        } else {
          quoted = false;
        }
      } else {
        cell += c;
      }
    } else if (c === '"' && atStart) {
      quoted = true;
      atStart = false;
    } else if (c === delim) {
      row.push(cell);
      cell = "";
      atStart = true;
    } else if (c === "\n" || c === "\r") {
      if (c === "\r" && text[i + 1] === "\n") i += 1;
      row.push(cell);
      rows.push(row);
      row = [];
      cell = "";
      atStart = true;
    } else {
      cell += c;
      atStart = false;
    }
  }
  if (cell !== "" || row.length) {
    row.push(cell);
    rows.push(row);
  }
  return rows;
}

// Reads a file that Excel (or Sheets) may have saved: UTF-8 with or without a
// BOM, UTF-16 ("Unicode Text") or the Turkish ANSI page.
function readTextFile(file) {
  const buf = fs.readFileSync(file);
  if (buf[0] === 0xff && buf[1] === 0xfe) return new TextDecoder("utf-16le").decode(buf.subarray(2));
  if (buf[0] === 0xfe && buf[1] === 0xff) return new TextDecoder("utf-16be").decode(buf.subarray(2));
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(buf).replace(/^﻿/, "");
  } catch {
    return new TextDecoder("windows-1254").decode(buf);
  }
}

// ---- Export --------------------------------------------------------------------

function placesOf(text) {
  return [...text.matchAll(/\{(\d+)\}/g)].map((m) => Number(m[1])).sort((a, b) => a - b);
}

function loadTable(srcFile) {
  const src = fs.readFileSync(srcFile, "utf8");
  const entries = parseTable(src);
  const open = lineOf(src, src.indexOf("table! {"));
  // The table ends at the first line that is only "}" after the opening.
  const lines = src.split("\n");
  let close = open;
  while (close < lines.length && lines[close] !== "}") close += 1;
  return { src, entries, tableLines: [open, close + 1] };
}

function writeFile(file, text, bom) {
  fs.mkdirSync(path.dirname(path.resolve(file)), { recursive: true });
  fs.writeFileSync(file, (bom ? "﻿" : "") + text, "utf8");
}

function exportTexts(outTsv, outMd, srcFile) {
  const { entries, tableLines } = loadTable(srcFile);
  const uses = findUses(entries.map((e) => e.key), tableLines);
  const rows = entries.map((e) => {
    const u = uses.get(e.key);
    const used = u.code.length
      ? u.code.join(", ")
      : u.tests.length
        ? `unused (tests only: ${u.tests.join(", ")})`
        : "unused";
    return { ...e, screen: screenOf(e.key, e.section), used, isUnused: u.code.length === 0 };
  });

  const head = ["Key", "Screen", "Türkçe", "English", "Used in", "Note"];
  const tsv = [head, ...rows.map((r) => [r.key, r.screen, r.tr.text, r.en.text, r.used, r.note])]
    .map((cells) => cells.map(quoteCell).join("\t"))
    .join("\r\n");
  writeFile(outTsv, tsv + "\r\n", true);

  if (outMd) {
    const cell = (s) => s.replace(/\|/g, "\\|").replace(/\r?\n/g, "<br>");
    const out = [];
    out.push("# Hexsight texts", "");
    out.push(
      `${rows.length} texts, ${rows.filter((r) => !r.isUnused).length} in use, ${rows.filter((r) => r.isUnused).length} unused. ` +
        "Placeholders such as {0} are filled by the program and must stay in both languages.",
      "",
    );
    const screens = [...SCREENS];
    for (const r of rows) if (!screens.includes(r.screen)) screens.push(r.screen);
    for (const screen of screens) {
      const part = rows.filter((r) => r.screen === screen);
      if (!part.length) continue;
      out.push(`## ${screen}`, "", "| Key | Türkçe | English |", "| --- | --- | --- |");
      for (const r of part) {
        out.push(`| \`${r.key}\`${r.isUnused ? " (unused)" : ""} | ${cell(r.tr.text)} | ${cell(r.en.text)} |`);
      }
      out.push("");
    }
    writeFile(outMd, out.join("\n"), false);
  }

  const unused = rows.filter((r) => r.isUnused).length;
  console.log(`${rows.length} keys: ${rows.length - unused} used, ${unused} unused`);
  console.log(`wrote ${outTsv}`);
  if (outMd) console.log(`wrote ${outMd}`);
}

// ---- Import --------------------------------------------------------------------

function importTexts(inFile, srcFile, dryRun) {
  const { src, entries } = loadTable(srcFile);
  const byKey = new Map(entries.map((e) => [e.key, e]));

  const text = readTextFile(inFile);
  const firstLine = text.split(/\r?\n/, 1)[0];
  const delim = ["\t", ";", ","]
    .map((d) => [d, firstLine.split(d).length])
    .sort((a, b) => b[1] - a[1])[0][0];
  const rows = parseCells(text, delim).filter((r) => r.some((c) => c.trim() !== ""));
  if (!rows.length) throw new Error("the file is empty");

  // Header names match without case and without accents (Türkçe = turkce).
  const norm = (s) =>
    s.trim().toLowerCase().replace(/ı/g, "i").normalize("NFD").replace(/[\u0300-\u036f]/g, "");
  const head = rows[0].map(norm);
  const col = (...names) => head.findIndex((h) => names.includes(h));
  const iKey = col("key", "anahtar");
  const iTr = col("turkce", "tr", "turkish");
  const iEn = col("english", "en", "ingilizce");
  if (iKey < 0 || iTr < 0 || iEn < 0) {
    throw new Error(`the header needs the columns Key, Türkçe and English (found: ${rows[0].join(" | ")})`);
  }

  const errors = [];
  const changes = []; // { entry, lang, from, to }
  const seen = new Set();
  rows.slice(1).forEach((row, n) => {
    const where = `row ${n + 2}`;
    const key = (row[iKey] ?? "").trim();
    if (!key) return;
    const entry = byKey.get(key);
    if (!entry) {
      errors.push(`${where}: unknown key ${key}`);
      return;
    }
    if (seen.has(key)) errors.push(`${where}: key ${key} appears twice`);
    seen.add(key);
    const next = {
      tr: (row[iTr] ?? "").replace(/\r\n?/g, "\n"),
      en: (row[iEn] ?? "").replace(/\r\n?/g, "\n"),
    };
    for (const lang of ["tr", "en"]) {
      if (next[lang].trim() === "") errors.push(`${where}: ${key} has no ${lang === "tr" ? "Türkçe" : "English"} text`);
    }
    if (placesOf(next.tr).join() !== placesOf(next.en).join()) {
      errors.push(
        `${where}: ${key} fills different places in the two languages (Türkçe ${JSON.stringify(placesOf(next.tr))}, English ${JSON.stringify(placesOf(next.en))})`,
      );
    }
    for (const lang of ["tr", "en"]) {
      if (next[lang] !== entry[lang].text) changes.push({ entry, lang, from: entry[lang].text, to: next[lang] });
    }
  });
  const missing = entries.filter((e) => !seen.has(e.key)).map((e) => e.key);

  if (errors.length) {
    console.error(`${errors.length} problem(s), nothing written:`);
    for (const e of errors) console.error(`  ${e}`);
    process.exit(1);
  }

  // Replace the literals from the end of the file to the start, so that the
  // offsets of the earlier ones stay valid.
  let out = src;
  for (const c of [...changes].sort((a, b) => b.entry[b.lang].start - a.entry[a.lang].start)) {
    const span = c.entry[c.lang];
    out = out.slice(0, span.start) + encodeLiteral(c.to) + out.slice(span.end);
  }

  // Check: the new file parses, with the same keys in the same order, and every
  // text is what the file asked for.
  const after = parseTable(out);
  const want = new Map(changes.map((c) => [`${c.entry.key}/${c.lang}`, c.to]));
  const bad =
    after.length !== entries.length ||
    after.some((e, i) => {
      const old = entries[i];
      return (
        e.key !== old.key ||
        e.en.text !== (want.get(`${e.key}/en`) ?? old.en.text) ||
        e.tr.text !== (want.get(`${e.key}/tr`) ?? old.tr.text)
      );
    });
  if (bad) throw new Error("the check after the change failed, nothing written");

  for (const c of changes) {
    console.log(`${c.entry.key} [${c.lang === "tr" ? "Türkçe" : "English"}]`);
    console.log(`  was: ${c.from}`);
    console.log(`  now: ${c.to}`);
  }
  if (missing.length) console.log(`note: ${missing.length} key(s) are not in the file and stay as they are`);
  if (!changes.length) {
    console.log("no text changed, i18n.rs stays as it is");
    return;
  }
  if (dryRun) {
    console.log(`${changes.length} text(s) would change (dry run, nothing written)`);
    return;
  }
  fs.writeFileSync(srcFile, out, "utf8");
  console.log(`${changes.length} text(s) written to ${srcFile}`);
}

// ---- Command line -------------------------------------------------------------

function main() {
  const args = process.argv.slice(2);
  const flag = (name) => {
    const at = args.indexOf(name);
    if (at < 0) return undefined;
    return args.splice(at, 2)[1];
  };
  const dryRun = args.includes("--dry-run");
  if (dryRun) args.splice(args.indexOf("--dry-run"), 1);
  const md = flag("--md");
  const srcFile = flag("--src") ?? path.join(SRC_DIR, "i18n.rs");
  const [cmd, file] = args;
  if ((cmd !== "export" && cmd !== "import") || !file) {
    console.error("usage: node tools/texts.mjs export <out.tsv> [--md <out.md>] [--src <i18n.rs>]");
    console.error("       node tools/texts.mjs import <in.tsv> [--src <i18n.rs>] [--dry-run]");
    process.exit(2);
  }
  if (cmd === "export") exportTexts(file, md, srcFile);
  else importTexts(file, srcFile, dryRun);
}

try {
  main();
} catch (e) {
  console.error(e.message);
  process.exit(1);
}
