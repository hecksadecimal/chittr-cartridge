# xtp (offline, Node-only)

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

ok so this is the offline xtp kit. we vendored the bindgen templates so the cli can render a scaffold with no network
and no wasm build. all the pinned commits and licenses live in PROVENANCE.md, go check em
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

a vendored copy of the xtp bindgen ecosystem plus a Node-only runner that renders any supported language's
binding/project-scaffold templates from an XTP schema YAML. no `xtp` CLI, no Extism wasm build, no network at use time.

## Layout

```
tools/xtp/
  bin/xtp-gen.mjs       CLI: render a language template from a schema
  bin/prune-pdk.mjs     helper: prune a Rust pdk.rs to a guest's lib.rs exports
  dist/*.cjs            committed esbuild bundles (one per language + core + js-yaml)
  vendor/xtp-bindgen/          pinned core (BSD-3-Clause)
  vendor/xtp-<lang>-bindgen/   pinned backends (BSD-3-Clause)
  build/                       build-time only: package.json + build-bundles.mjs
  PROVENANCE.md                repos, pinned commits, licenses, adaptations
```

see `PROVENANCE.md` for the pinned upstream commits and licenses.

## Usage

```sh
node tools/xtp/bin/xtp-gen.mjs --list-langs
node tools/xtp/bin/xtp-gen.mjs \
  --schema docs/cartridges/v1/server.xtp.yaml \
  --lang rust \
  --out /tmp/server-scaffold \
  --name myplugin
```

`--lang` is one of `rust`, `typescript`, `go`, `python`, `csharp`, `zig`, `cpp`.
every file under the backend's `template/` directory is emitted: a file ending
in `.ejs` is rendered as an EJS template (suffix stripped), every other file is
copied verbatim. optional `--description`, `--app-id` and `--extension-point-id`
feed the `project` fields the scaffold templates use.

at use time only Node is required. the `dist/*.cjs` bundles inline `ejs` and
`js-yaml`; `npm install` is needed only to rebuild them:

```sh
cd tools/xtp/build
npm install          # esbuild, ejs, js-yaml (network required)
node build-bundles.mjs
```

## How a backend runs without Extism

each upstream backend is an Extism JS plugin whose `src/index.ts` exports
`render()`: it reads an EJS template from `Host.inputString()`, builds a context
with the core's `getContext()` (which reads `{schema, project, ...}` JSON from
`Config.get("ctx")`), renders with EJS, and writes `Host.outputString()`.

`build-bundles.mjs` bundles that `render()` into CommonJS with esbuild, aliasing
`@dylibso/xtp-bindgen` to the vendored core. `xtp-gen.mjs` then shims the two
Extism globals in Node and calls `render()` once per template file.

## Two details that matter for fidelity

1. **Deep key sort.** the real `xtp` CLI parses the schema YAML into Go maps and
   hands it to the plugin as JSON; Go's `encoding/json` sorts map keys, so the
   plugin sees the schema with every object's keys alphabetically sorted.
   `xtp-gen.mjs` reproduces this (`deepSortKeys`) before rendering. without it,
   Rust types/imports come out in YAML order and never match `xtp plugin init`.

2. **`free-form-object` compat.** the pinned core turns a bare `type: object`
   into a `free-form-object` type that only the pinned Rust backend understands.
   the six other backends predate that kind and render untyped objects through an
   empty `ObjectType`. `build/core-compat.ts` rewrites `free-form-object` to that
   empty-object shape for every non-Rust bundle; Rust keeps the pristine core.

## Known upstream limitations

- **TypeScript `format: int64`.** the pinned upstream backend throws
  `We do not support format int64 yet`, but the vendored copy is patched (see
  `PROVENANCE.md`, adaptation 3): `int64` maps to `number`, the same as `int32`,
  because every `int64` field in the Chittr schemas is a safe JSON integer
  (within 2^53), as `PROVENANCE.md` and the schema descriptions note. no schema
  change or ABI encoding change is needed, and the v1 and v2 server and canvas
  schemas scaffold for TypeScript. all seven backends render clean.

## Regenerating the committed Rust bindings

`scripts/gen-bindings.sh` drives the toolkit through the same pipeline
`xtp plugin init` uses -- render, `rustfmt`, then prune the `#[no_mangle]`
wrappers the guest's `src/lib.rs` does not define (mirroring `cli/src/sync.rs`).
it is a dry run by default:

```sh
scripts/gen-bindings.sh                     # v1, both guests, diff vs committed
scripts/gen-bindings.sh --version v2        # render from docs/cartridges/v2
scripts/gen-bindings.sh --write             # overwrite cli/templates/*/src/pdk.rs
scripts/gen-bindings.sh --out /tmp/out      # write finalized pdk.rs elsewhere
```
