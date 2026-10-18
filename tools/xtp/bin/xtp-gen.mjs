#!/usr/bin/env node
// xtp-gen: render an xtp bindgen template against an XTP schema, offline, with Node only.
//
// This is a drop-in replacement for the parts of `xtp plugin init` that render a
// language template from a schema. It does not build a wasm plugin and never
// touches the network. Each vendored backend is a committed esbuild bundle of the
// upstream `src/index.ts` (the Extism entry point) with the Extism globals shimmed
// here in Node, plus the upstream `template/` directory of EJS files.
//
// Usage:
//   node tools/xtp/bin/xtp-gen.mjs --schema <schema.yaml> --lang <lang> --out <dir> [--name <plugin-name>]
//   node tools/xtp/bin/xtp-gen.mjs --list-langs
//
// A file whose name ends in `.ejs` is rendered as an EJS template and written
// without the suffix; every other file is copied verbatim.
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";
import fs from "node:fs";
import path from "node:path";

const require = createRequire(import.meta.url);
const HERE = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(HERE, ".."); // tools/xtp
const DIST = path.join(ROOT, "dist");
const VENDOR = path.join(ROOT, "vendor");

// lang -> vendored backend directory. Keep in sync with build/build-bundles.mjs.
const BACKENDS = {
  rust: "xtp-rust-bindgen",
  typescript: "xtp-typescript-bindgen",
  go: "xtp-go-bindgen",
  python: "xtp-python-bindgen",
  csharp: "xtp-csharp-bindgen",
  zig: "xtp-zig-bindgen",
  cpp: "xtp-cpp-bindgen",
};

const LANGS = Object.keys(BACKENDS).sort();

function usage() {
  return [
    "Usage:",
    "  xtp-gen --schema <schema.yaml> --lang <lang> --out <dir> [--name <plugin-name>]",
    "  xtp-gen --list-langs",
    "",
    "Options:",
    "  --schema <file>   XTP schema YAML (version v0 or v1-draft)",
    "  --lang <lang>     Target language: " + LANGS.join(", "),
    "  --out <dir>       Output directory (created if missing)",
    "  --name <name>     Plugin/project name used by the scaffold templates",
    "  --description <d> Project description used by the scaffold templates",
    "  --app-id <id>         app_id written to xtp.toml",
    "  --extension-point-id <id>  extension_point_id written to xtp.toml",
    "  --list-langs      Print the supported languages and exit",
  ].join("\n");
}

function die(msg) {
  process.stderr.write(`xtp-gen: error: ${msg}\n`);
  process.exit(1);
}

function parseArgs(argv) {
  const opts = { flags: {} };
  for (let i = 0; i < argv.length; i++) {
    const arg = argv[i];
    if (arg === "--list-langs") {
      opts.listLangs = true;
    } else if (arg === "-h" || arg === "--help") {
      opts.help = true;
    } else if (arg.startsWith("--")) {
      const key = arg.slice(2);
      const value = argv[++i];
      if (value === undefined || value.startsWith("--")) {
        die(`missing value for --${key}`);
      }
      opts[key] = value;
    } else {
      die(`unexpected argument: ${arg}`);
    }
  }
  return opts;
}

// The Go `xtp` CLI parses the schema YAML into Go maps and passes it to the
// bindgen plugin as JSON. `encoding/json` marshals Go maps with their keys
// sorted, so the plugin sees every object with alphabetically sorted keys.
// Reproduce that here so the generated output is byte-identical to `xtp`.
function deepSortKeys(value) {
  if (Array.isArray(value)) return value.map(deepSortKeys);
  if (value && typeof value === "object") {
    const out = {};
    for (const key of Object.keys(value).sort()) out[key] = deepSortKeys(value[key]);
    return out;
  }
  return value;
}

function listFiles(dir, prefix = "") {
  const out = [];
  for (const entry of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const rel = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.isDirectory()) out.push(...listFiles(path.join(dir, entry.name), rel));
    else if (entry.isFile()) out.push(rel);
  }
  return out;
}

function main() {
  const opts = parseArgs(process.argv.slice(2));

  if (opts.help) {
    process.stdout.write(usage() + "\n");
    return;
  }
  if (opts.listLangs) {
    process.stdout.write(LANGS.join("\n") + "\n");
    return;
  }

  if (!opts.schema) die("--schema is required\n\n" + usage());
  if (!opts.lang) die("--lang is required\n\n" + usage());
  if (!opts.out) die("--out is required\n\n" + usage());

  const lang = opts.lang;
  const backendDir = BACKENDS[lang];
  if (!backendDir) {
    die(`unknown language '${lang}'. Supported languages: ${LANGS.join(", ")}`);
  }

  const schemaPath = path.resolve(opts.schema);
  if (!fs.existsSync(schemaPath)) die(`schema not found: ${schemaPath}`);

  const backendBundle = path.join(DIST, `${lang}.cjs`);
  const coreBundle = path.join(DIST, "core.cjs");
  const yamlBundle = path.join(DIST, "js-yaml.cjs");
  const templateDir = path.join(VENDOR, backendDir, "template");
  for (const [what, p] of [["backend bundle", backendBundle], ["core bundle", coreBundle], ["yaml bundle", yamlBundle], ["template dir", templateDir]]) {
    if (!fs.existsSync(p)) {
      die(`${what} missing: ${p}\nRun 'npm install && npm run build' in tools/xtp/build to regenerate the committed bundles.`);
    }
  }

  const yaml = require(yamlBundle);
  let schema;
  try {
    schema = yaml.load(fs.readFileSync(schemaPath, "utf8"));
  } catch (err) {
    die(`could not parse YAML in ${schemaPath}: ${err.message}`);
  }
  if (!schema || typeof schema !== "object" || Array.isArray(schema)) {
    die(`${schemaPath}: schema must be a mapping at the top level`);
  }

  // Validate with the vendored core before rendering anything. Accepts
  // `version: v0` and `version: v1-draft`.
  const core = require(coreBundle);
  try {
    core.parse(JSON.stringify(deepSortKeys(schema)));
  } catch (err) {
    const details = Array.isArray(err.errors)
      ? "\n" + err.errors.map((e) => `  ${e.path ?? ""}: ${e.message}`).join("\n")
      : "";
    die(`schema ${schemaPath} is invalid: ${err.message}${details}`);
  }

  const out = opts.name || path.basename(path.resolve(opts.out));
  const ctx = {
    schema: deepSortKeys(schema),
    project: {
      name: out,
      description: opts.description ?? "",
      appId: opts["app-id"] ?? "",
      extensionPointId: opts["extension-point-id"] ?? "",
    },
    featureFlags: {},
  };

  // Shim the two Extism globals the backend reads. `render()` is called once per
  // template file; it reads the source from Host.inputString() and writes the
  // result to Host.outputString().
  let template = "";
  let rendered = "";
  globalThis.Host = {
    inputString: () => template,
    outputString: (s) => {
      rendered = s;
    },
  };
  globalThis.Config = {
    get: (key) => (key === "ctx" ? JSON.stringify(ctx) : ""),
  };

  const backend = require(backendBundle);
  if (typeof backend.render !== "function") {
    die(`backend '${lang}' did not export render()`);
  }

  const outDir = path.resolve(opts.out);
  const files = listFiles(templateDir);
  let written = 0;
  for (const rel of files) {
    const src = path.join(templateDir, rel);
    const relOut = rel.endsWith(".ejs") ? rel.slice(0, -".ejs".length) : rel;
    const dest = path.join(outDir, relOut);
    fs.mkdirSync(path.dirname(dest), { recursive: true });
    if (rel.endsWith(".ejs")) {
      template = fs.readFileSync(src, "utf8");
      rendered = "";
      try {
        backend.render();
      } catch (err) {
        die(`rendering ${backendDir}/template/${rel} for '${lang}' failed: ${err.stack || err.message}`);
      }
      fs.writeFileSync(dest, rendered);
    } else {
      fs.copyFileSync(src, dest);
    }
    written++;
  }
  process.stdout.write(`xtp-gen: ${lang}: wrote ${written} file(s) to ${outDir}\n`);
}

main();
