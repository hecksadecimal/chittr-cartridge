# Snake (example Cartridge)

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

snake, but make it a chitt. the canvas does the game, the server keeps the leaderboard. go nuts
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

a keyboard- and touch-playable Snake game with a top-five leaderboard, ranked by score and labeled with each player's
trolltag. the Server Guest (`src/lib.rs`) keeps each Observer's best score and draws the leaderboard; the Canvas Guest
(`canvas/src/lib.rs`) runs the game and sends finished scores to the Server Guest.

both guests' bindings are generated from the XTP schemas in `docs/cartridges/v1/` with `xtp plugin init` (the stock Rust
template), not written by hand.

## Scaffolding

run from the repository root. each `xtp plugin init` writes into a scratch folder, and `chittr-cartridge sync` copies its
`src/pdk.rs` into this example. `src/lib.rs` and `canvas/src/lib.rs` (the author's files) are left alone:

```bash
SCRATCH=$(mktemp -d)

# Server Guest, from the Server schema
xtp plugin init --schema-file docs/cartridges/v1/server.xtp.yaml --template Rust --path "$SCRATCH/server" --name snake -y
chittr-cartridge sync "$SCRATCH/server" examples/snake

# Canvas Guest, from the Canvas schema
xtp plugin init --schema-file docs/cartridges/v1/canvas.xtp.yaml --template Rust --path "$SCRATCH/canvas" --name snake-canvas -y
chittr-cartridge sync "$SCRATCH/canvas" examples/snake/canvas
```

`chittr-cartridge sync` removes the wrappers for the Hooks each guest doesn't define, as the poll's README describes. the Server
Guest defines `init`, `render` and `on_canvas_message`, so sync removes the rest (`migrate`, `on_control`, `on_reaction`,
`on_rechitt`, `on_reply`, `on_timer` and `on_view`). the Canvas Guest exports `start`, `frame` and `event`, and all three stay.

the `Cargo.toml` files are the template's, with the additions in the poll's README: chrono without default features, a
`[profile.release]`, and in `canvas/Cargo.toml` the SDK's `canvas` feature for the command-buffer encoder (`Canvas2D`).

## Building and packaging

```bash
sh scripts/build-examples.sh          # from the repository root: builds every example
chittr-cartridge pack examples/snake/cartridge   # writes examples/snake/snake.cartridge
```

`chittr-cartridge pack` checks the folder and packs it, as the command above shows. to test the result on Chittr, see the
repository README.

## Files

- `src/lib.rs`, `canvas/src/lib.rs`: the leaderboard and the game (author's files).
- `src/pdk.rs`, `canvas/src/pdk.rs`: generated bindings, pruned to the Hooks each guest defines.
- `cartridge/`: `manifest.json`, `templates/card.liquid`, and the built `server.wasm` and `canvas.wasm` (written by the build,
  not committed).
