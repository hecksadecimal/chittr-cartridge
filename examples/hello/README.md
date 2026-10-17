# Hello (example Cartridge)

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

the smallest cartridge in the box. init gives the chitt its "Hello!", render says hi to whoever's looking. no canvas, no fuss. start here
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

the smallest Server Guest: `init` gives the chitt the text "Hello!", and `render` passes the current Observer to the card, which
greets them by name (or says "Hello, stranger!" when logged out). it has no Canvas Guest.

the bindings in `src/pdk.rs` are generated from `docs/cartridges/v1/server.xtp.yaml` with `xtp plugin init` (the stock Rust
template), not written by hand. `chittr-cartridge sync` copies them in and prunes the Hooks this example doesn't define, so the
wasm exports exactly `init` and `render`.

## Scaffolding

run from the repository root:

```bash
SCRATCH=$(mktemp -d)
xtp plugin init --schema-file docs/cartridges/v1/server.xtp.yaml --template Rust --path "$SCRATCH/server" --name hello -y
chittr-cartridge sync "$SCRATCH/server" examples/hello
```

`cartridge` holds the card, the manifest and the built wasm. `Cargo.toml` is the template's, with the two edits the quickstart
describes: chrono without its default features, and a `[profile.release]`.

## Building and packaging

```bash
sh scripts/build-examples.sh                  # from the repository root: builds every example
chittr-cartridge check examples/hello/cartridge
chittr-cartridge pack examples/hello/cartridge   # writes examples/hello/hello.cartridge
```

to test the result on Chittr, see the repository README.

## Files

- `src/lib.rs`: the Hooks (author's file).
- `src/pdk.rs`: generated bindings, pruned to the Hooks the Cartridge defines.
- `cartridge/`: `manifest.json` and `templates/card.liquid`, plus the built `server.wasm` (written by the build, not committed).
