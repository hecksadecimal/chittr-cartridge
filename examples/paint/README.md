# Collaborative painting (example Cartridge)

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

the shared pixel board. everybody paints on the same canvas, and the server remembers it all. pls be nice to the board
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

a shared 512-by-512 pixel-art canvas. the Server Guest (`src/lib.rs`) stores the board per chitt and pushes changes to every
viewer. the Canvas Guest (`canvas/src/lib.rs`) is the painting tool: strokes are optimistic locally and sent to the Server Guest
in small batches.

both guests' bindings are generated from the XTP schemas in `docs/cartridges/v1/` with `xtp plugin init` (the stock Rust
template), not written by hand.

## Scaffolding

run from the repository root. each `xtp plugin init` writes into a scratch folder, and `chittr-cartridge sync` copies its
`src/pdk.rs` into this example. `src/lib.rs` and `canvas/src/lib.rs` (the author's files) are left alone:

```bash
SCRATCH=$(mktemp -d)

# Server Guest, from the Server schema
xtp plugin init --schema-file docs/cartridges/v1/server.xtp.yaml --template Rust --path "$SCRATCH/server" --name paint -y
chittr-cartridge sync "$SCRATCH/server" examples/paint

# Canvas Guest, from the Canvas schema
xtp plugin init --schema-file docs/cartridges/v1/canvas.xtp.yaml --template Rust --path "$SCRATCH/canvas" --name paint-canvas -y
chittr-cartridge sync "$SCRATCH/canvas" examples/paint/canvas
```

`chittr-cartridge sync` removes the wrappers for the Hooks each guest doesn't define, as the poll's README describes. the Server
Guest defines `init`, `render` and `on_canvas_message`, so sync removes the rest (`migrate`, `on_control`, `on_reaction`,
`on_rechitt`, `on_reply`, `on_timer` and `on_view`). the Canvas Guest exports `start`, `frame` and `event`, and all three stay.

the `Cargo.toml` files are the template's, with the additions in the poll's README: chrono without default features, a
`[profile.release]`, and in `canvas/Cargo.toml` the SDK's `canvas` feature for the command-buffer encoder (`Canvas2D`).

the Server Guest keeps its board in the KV store through a small local module (`kv`, and `push` for Render Pushes) over the
generated bindings. it takes the same results and errors the board code was written against, so the board logic is unchanged.
the Canvas Guest reads each event's fields from the generated `CanvasEventInput` as one JSON object, which is how its handlers
were written.

## Building and packaging

```bash
sh scripts/build-examples.sh          # from the repository root: builds every example
chittr-cartridge pack examples/paint/cartridge   # writes examples/paint/paint.cartridge
```

`chittr-cartridge pack` checks the folder and packs it, as the command above shows. to test the result on Chittr, see the
repository README.

## Files

- `src/lib.rs`, `canvas/src/lib.rs`: the board and the painting tool (author's files).
- `src/pdk.rs`, `canvas/src/pdk.rs`: generated bindings, pruned to the Hooks each guest defines.
- `cartridge/`: `manifest.json`, `templates/card.liquid`, and the built `server.wasm` and `canvas.wasm` (written by the build,
  not committed).
