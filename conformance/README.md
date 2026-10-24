# Conformance corpus

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

ok so this is the test pile. every rule the validator enforces has at least one case in here, good and bad. run them all
and see who's still standing
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

the corpus is a set of small cartridges, and the result each one must get from the validator. the `chittr-cartridge` CLI
runs every case in its tests (`cargo test -p chittr-cartridge`). any other validator can run the same cases too, and that's
how a host's validator gets checked against the same rules.

## Layout

```
conformance/
  README.md
  cases/
    <case-id>/
      expected.json      the result the validator must return
      files/             the Cartridge's files, at the archive's root   (or)
      archive.zip        the archive itself, byte for byte
```

a case has exactly one of `files/` or `archive.zip`:

- `files/` is a directory tree. pack it into a zip with its files at the root, no enclosing folder. use deflate (method 8)
  or stored (method 0) for each entry, and write no directory entries. dotfiles aren't part of a Cartridge, so skip them.
- `archive.zip` is for layouts a normal packer won't write: directory entries, bad paths, wrong methods, understated sizes,
  ZIP64 markers, and corrupt headers. feed these bytes to the validator as they are.

## expected.json

```json
{ "ok": true }
{ "ok": false, "error": "manifest.json: is missing \"v\"" }
{ "ok": false, "error": "canvas.wasm: is not a valid WebAssembly module: *" }
{ "ok": true, "limits": { "archiveBytes": 485 } }
```

- `ok` is required. `true` means the validator must accept the archive.
- `error` is required when `ok` is `false`. it's the `file: rule` the validator reports, the same text the server shows. the
  file is the archive entry the rule is about, or `cartridge` for a rule about the whole archive.
- a trailing `*` in `error` matches a prefix. use it where the text comes from a decoder or a compiler (an inflate error, a
  WebAssembly compile error, a LiquidJS parse error). those messages differ between engines, and the part that matters is the
  file and the rule's first words.
- `limits` is optional. each key overrides one of the validator's limits, using the server's limit names: `archiveBytes`,
  `inflatedBytes`, `assetBytes`, `wasmBytes`, `files`, `templateFiles`, `templateBytes`, `manifestBytes`, `stylesheetBytes`. a limit gets lowered
  so its rule can be hit with a small archive. every other limit keeps its default.

## Defaults

the validator's limits are the server's: 32 MiB archive, 64 MiB inflated, 16 MiB per asset, 4 MiB per wasm, 1000 files, 64
templates, 256 KiB of templates, 16 KiB of manifest, 64 KiB of `style.css`. a canvas guest declares at most 1024 pages of memory (64 MiB).

## What the cases cover

- layout: the allowed root files (`style.css` among them), `templates/*.liquid`, `assets/**`, and the directory entries;
- archive: paths, duplicates, encryption, compression methods, ZIP64, a corrupt central directory, corrupt local headers, data
  that runs past the end, stored sizes, understated and overstated deflate sizes, and a zip bomb;
- limits: archive, file count, inflated total, manifest, wasm, asset, template count and template bytes, `style.css`, and the
  template name length;
- manifest: JSON shape, unknown keys, `v`, `name` and `version`, `fonts`;
- controls: declarations, enums, fields, and template use of undeclared or dynamic Control names;
- templates: UTF-8, LiquidJS parse, the canvas placement rules, and the v2-only badge elements;
- stylesheet: `style.css`'s UTF-8. its CSS rules and the Tailwind compile run on the host, not in the validator;
- server exports: the Hooks, `render`, and the rule against other exports;
- canvas exports: `start`, `frame`, optional `event`, `memory`, and the memory cap for both declared and imported memory.

## Regenerating

the corpus is generated from a table in `cli/tests/regen_conformance.rs`, which is ignored by default:

```bash
cargo test -p chittr-cartridge --test regen_conformance -- --ignored
```

its results were checked against the server's validator, so the expected results are the server's own. after a change, run
the CLI's tests, which run every case: `cargo test -p chittr-cartridge --test conformance`.
