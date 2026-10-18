// Build the Node-only bundles for every vendored xtp bindgen backend.
//
// Upstream each backend is an Extism JS plugin: `src/index.ts` exports `render()`,
// which reads the EJS template from `Host.inputString()`, builds a context with
// `getContext()` (which reads the schema/project JSON from `Config.get("ctx")`),
// renders with EJS, and writes `Host.outputString()`. To run without Extism we
// bundle `src/index.ts` into a CommonJS file and shim the two globals in Node.
//
// The `@dylibso/xtp-bindgen` import is aliased to the vendored core in
// ../vendor/xtp-bindgen so the pinned commit is what actually renders.
//
// Run:  node build-bundles.mjs
import * as esbuild from "esbuild";
import { fileURLToPath } from "node:url";
import path from "node:path";
import fs from "node:fs";

const here = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(here, ".."); // tools/xtp
const VENDOR = path.join(ROOT, "vendor");
const DIST = path.join(ROOT, "dist");
const NODE_MODULES = path.join(here, "node_modules");
const CORE_ENTRY = path.join(VENDOR, "xtp-bindgen", "src", "index.ts");
// A thin shim over the pinned core that maps `free-form-object` types to the
// empty-object shape the non-Rust backends expect. Rust keeps the pristine core.
const CORE_COMPAT_ENTRY = path.join(here, "core-compat.ts");

// The Go `xtp` CLI predates the core's `free-form-object` kind. Rust is the only
// vendored backend that understands it, so it is the only one that uses the
// pristine core; everything else renders the same untyped JSON through the
// empty-object path it already had.
const COMPAT_LANGS = new Set(["typescript", "go", "python", "csharp", "zig", "cpp"]);

// lang -> vendored backend directory
export const BACKENDS = {
  rust: "xtp-rust-bindgen",
  typescript: "xtp-typescript-bindgen",
  go: "xtp-go-bindgen",
  python: "xtp-python-bindgen",
  csharp: "xtp-csharp-bindgen",
  zig: "xtp-zig-bindgen",
  cpp: "xtp-cpp-bindgen",
};

fs.mkdirSync(DIST, { recursive: true });

const common = {
  bundle: true,
  format: "cjs",
  platform: "node",
  target: "node20",
  logLevel: "warning",
  nodePaths: [NODE_MODULES],
  alias: { "@dylibso/xtp-bindgen": CORE_ENTRY },
};

// The core, bundled on its own, so bin/xtp-gen.mjs can validate a schema up
// front (and report NormalizeError details) before rendering any file.
await esbuild.build({
  ...common,
  entryPoints: [CORE_ENTRY],
  outfile: path.join(DIST, "core.cjs"),
});
console.log(`built dist/core.cjs (${fs.statSync(path.join(DIST, "core.cjs")).size} bytes)`);

for (const [lang, dir] of Object.entries(BACKENDS)) {
  const entry = path.join(VENDOR, dir, "src", "index.ts");
  const outfile = path.join(DIST, `${lang}.cjs`);
  const alias = {
    "@dylibso/xtp-bindgen": COMPAT_LANGS.has(lang) ? CORE_COMPAT_ENTRY : CORE_ENTRY,
  };
  await esbuild.build({ ...common, alias, entryPoints: [entry], outfile });
  const size = fs.statSync(outfile).size;
  console.log(`built dist/${lang}.cjs (${size} bytes)`);
}

// js-yaml is needed at use time by bin/xtp-gen.mjs to read the schema YAML.
// Bundle it so only Node is required at use time.
const yamlEntry = path.join(here, "js-yaml-entry.cjs");
fs.writeFileSync(yamlEntry, "module.exports = require('js-yaml');\n");
await esbuild.build({
  ...common,
  entryPoints: [yamlEntry],
  outfile: path.join(DIST, "js-yaml.cjs"),
});
fs.rmSync(yamlEntry);
console.log(`built dist/js-yaml.cjs (${fs.statSync(path.join(DIST, "js-yaml.cjs")).size} bytes)`);
