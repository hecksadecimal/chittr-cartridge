# Provenance

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

ok so this is the receipts page. every vendored repo, the exact commit we pinned, its license, and the one little
patch we made. keep it honest c;
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

`tools/xtp/vendor/` holds source copies of the upstream xtp bindgen ecosystem, vendored at the pinned commits below.
each directory is the upstream repository tree at that commit with its `.git` directory removed. one vendored source
file is patched (the TypeScript backend's `int64` mapping, adaptation 3 below); everything else is unmodified.
`node_modules/` and build output are not vendored.

every component is BSD-3-Clause. where the upstream repository ships a `LICENSE` file it's kept in place; the two
repositories that don't ship one declare `"license": "BSD-3-Clause"` in their `package.json`:

| Component | Repository | Pinned commit | License file |
|---|---|---|---|
| core | https://github.com/dylibso/xtp-bindgen | `237aa107946d54349545ce04f9f4b71c87d07c8d` | `vendor/xtp-bindgen/LICENSE` |
| rust | https://github.com/dylibso/xtp-rust-bindgen | `adc3ba2fb38ebce49efac10bd327ffad9edab2e3` | none upstream (`package.json`: BSD-3-Clause) |
| typescript | https://github.com/dylibso/xtp-typescript-bindgen | `ee2e58916ffd79946f61360ee39b9fb038e9577a` | `vendor/xtp-typescript-bindgen/LICENSE` |
| go | https://github.com/dylibso/xtp-go-bindgen | `bf5b1c274e8e335b2f90d290159e399694e2449b` | `vendor/xtp-go-bindgen/LICENSE` |
| python | https://github.com/dylibso/xtp-python-bindgen | `fd1d611f8a0597127db3af81550da56d03001ed7` | none upstream (`package.json`: BSD-3-Clause) |
| csharp | https://github.com/dylibso/xtp-csharp-bindgen | `617993a6410790747923208ec741a1e95cdc4e73` | `vendor/xtp-csharp-bindgen/LICENSE` |
| zig | https://github.com/dylibso/xtp-zig-bindgen | `2c58c3252fdc229e48d99d0efa344d2ae2c8e41b` | `vendor/xtp-zig-bindgen/LICENSE` |
| cpp | https://github.com/dylibso/xtp-cpp-bindgen | `ca2c8fb9a8efe2b2b6749e3fd48612f1ee470e8b` | `vendor/xtp-cpp-bindgen/LICENSE` |

all pinned commits were the default-branch HEAD at vendoring time.

## How the copies were made

for each component, with network access (build time only):

```sh
git init <dir> && cd <dir>
git remote add origin https://github.com/dylibso/<repo>
git fetch --depth 1 origin <commit>
git checkout FETCH_HEAD
rm -rf .git
```

## Adaptations (not in `vendor/`)

the vendored trees are upstream. two small, documented steps live in
`tools/xtp/build/` and shape the committed bundles:

1. **`build-bundles.mjs`** aliases every backend's `@dylibso/xtp-bindgen`
   import to the vendored core (`vendor/xtp-bindgen/src/index.ts`), so the
   pinned core is what renders instead of the npm-published package. it also
   bundles `ejs` and `js-yaml`, so `dist/` needs only Node at use time.

2. **`core-compat.ts`** is used for the six non-Rust backends. the pinned core
   normalizes a bare `type: object` ("any JSON") to a `free-form-object` type
   that only the pinned Rust backend understands; the other backends predate
   it and expect an empty `ObjectType`, which they already render as `any` /
   `interface{}` / `dict` / `JsonNode` / `jsoncons::json`. the compat module
   re-exports the vendored core unchanged except for `getContext()`, which
   rewrites `free-form-object` types into that empty-object shape. Rust uses
   the pristine core so its output stays byte-identical to `xtp plugin init`.

3. **`vendor/xtp-typescript-bindgen/src/index.ts`** (`int64`) is patched. the
   upstream TypeScript backend throws `We do not support format int64 yet`, but
   the Chittr schemas use `int64` (file handles, KV counters). the patch maps
   `int64` to `number`, exactly as `int32`/`float`/`double` already are. every
   `int64` field in these schemas is a safe integer (within 2^53), which is what
   `docs/cartridges/*/abi.md` already requires of the wire values. this is the
   only vendored source change; the rebuilt `dist/typescript.cjs` carries it.

## Generated / committed bundles

`tools/xtp/dist/*.cjs` are committed esbuild bundles of the vendored backends
(plus the core and js-yaml), built by `tools/xtp/build/build-bundles.mjs`.
`tools/xtp/build/package-lock.json` pins the build-time dependencies
(`esbuild`, `ejs`, `js-yaml`); `npm install` is only needed to rebuild the
bundles, never to use `tools/xtp/bin/xtp-gen.mjs`.
