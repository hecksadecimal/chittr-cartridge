#!/usr/bin/env node
// Prune an `xtp plugin init` Rust scaffold's src/pdk.rs down to the exports a
// Cartridge's src/lib.rs actually defines.
//
// This mirrors `chittr_cartridge_cli::sync` (cli/src/sync.rs): the stock
// template emits a `#[no_mangle]` wrapper for every export in the schema, and
// `sync` removes the wrappers for Hooks the project does not implement. The
// committed cli/templates/*/src/pdk.rs files are pruned, so regenerating them
// byte-for-byte needs the same step.
//
// Usage:
//   node prune-pdk.mjs --scaffold <dir> --lib <lib.rs> --out <pdk.rs>
import fs from "node:fs";
import path from "node:path";

function usage() {
  return "Usage: prune-pdk.mjs --scaffold <dir> --lib <lib.rs> --out <pdk.rs>";
}

function die(msg) {
  process.stderr.write(`prune-pdk: error: ${msg}\n`);
  process.exit(1);
}

const args = { _: [] };
for (let i = 2; i < process.argv.length; i++) {
  const a = process.argv[i];
  if (a.startsWith("--")) args[a.slice(2)] = process.argv[++i];
  else args._.push(a);
}

if (!args.scaffold || !args.lib || !args.out) die("--scaffold, --lib and --out are required\n" + usage());

const source = fs.readFileSync(path.join(args.scaffold, "src", "pdk.rs"), "utf8");

// Same regexes as cli/src/sync.rs.
const WRAPPER = /    #\[no_mangle\]\n    pub extern "C" fn (\w+)\(\) -> i32 \{\n/g;
const DEFINED = /^(?:pub(?:\(crate\))?\s+)?fn\s+(\w+)\s*[(<]/gm;
const WRAPPER_END = "\n    }\n";

const spans = [];
for (const m of source.matchAll(WRAPPER)) {
  const start = m.index;
  const after = start + m[0].length;
  const close = source.indexOf(WRAPPER_END, after);
  if (close < 0) die(`export ${m[1]} has no closing brace`);
  let end = close + WRAPPER_END.length;
  if (source[end] === "\n") end += 1;
  spans.push({ name: m[1], start, end });
}

const exported = spans.map((s) => s.name);
const kind = exported.includes("start") ? "canvas" : "server";

const libSource = fs.readFileSync(args.lib, "utf8");
const defined = new Set([...libSource.matchAll(DEFINED)].map((m) => m[1]));

const required = kind === "canvas" ? ["start", "frame"] : ["render"];
const missing = required.filter((name) => !defined.has(name));
if (missing.length > 0) {
  die(`${args.lib}: must define ${missing.join(", ")} (${kind} Guest)`);
}

let out = "";
let cursor = 0;
for (const span of spans) {
  if (defined.has(span.name)) continue;
  out += source.slice(cursor, span.start);
  cursor = span.end;
}
out += source.slice(cursor);

fs.mkdirSync(path.dirname(path.resolve(args.out)), { recursive: true });
fs.writeFileSync(args.out, out);
