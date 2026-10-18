# Breakout (example Cartridge)

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

ok breakout in a canvas, with leaderboards kept on the server. built it to show off what a canvas guest can do when u
let it run the whole game. watch ur step, the ball is fast
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

a neon Breakout clone in a Canvas Guest, with daily and all-time top-five leaderboards kept by the Server Guest. the
board is a 32×32 grid of square bricks; clear every breakable brick to advance. the Canvas Guest runs the whole game and
sends one finished score, and the Server Guest keeps each Character's best score on each board and orders ties by
trolltag.

## Gameplay

- **A 32×32 board of square bricks**: First Light, Checkmate, Bulwark, Diamond and Invader, each a hand-built layout. levels mix single-hit, tougher two- and three-hit bricks (marked with an inner plate, rivets and a crack overlay) and indestructible steel blocks that just bounce the ball.
- **A level editor for the poster**: the level set lives in the Server Guest's KV store. the poster sees an **EDIT** button in the canvas; the editor paints the 32×32 grid, and each brick can be **any colour** from an on-canvas hue-and-shade field, at strength 1, 2, 3 or steel. it adds or removes rows, adds levels (up to six), restores the defaults, **renames** a level (type on a keyboard, or use the on-screen keypad on touch), **saves** the level being edited (validated server-side and pushed to every viewer) and **playtests** it first. the grid renders each brick exactly as it appears in play. tap the level tabs to switch levels.
- **Fullscreen**: the card opts in with `<lc-canvas fullscreen>`, so the host draws the expand button. the guest keeps a fixed logical playfield and letterboxes it, so the board keeps its shape and scales up cleanly.
- **Power-ups** drop from broken bricks and are caught with the paddle:
  - **M** — multiball, splitting every ball (up to eight on screen),
  - **E** — a wider paddle for 16 seconds,
  - **L** — a laser with a limited magazine; each shot fires a pair of bolts (Space or the pointer) and consumes one round, and picking up another laser adds 15 rounds to what you already have,
  - **S** — slow-motion for 9 seconds,
  - **+** — two extra lives.
- **Combos**: chaining brick breaks multiplies the score up to x5. the multiplier resets when the ball meets the paddle.
- **Feel**: a starfield and nebula background, gradient bricks with cracks, glowing ball trails, sparks, confetti, screen shake and a soft edge-glow on a hit (no hard full-screen flashes).

controls: the board is inert until the first click or tap wakes it; after that, **Space** (or a second tap) launches the ball. steer with the arrow keys, A/D, or the pointer. Space also fires the laser, and restarts when a round ends. the poster can press **E** or tap **EDIT** to edit levels.

## Levels in the KV store

the Server Guest seeds `levels` on first publish and stores the poster's edits there. a level is a name, a small table of brick styles (health, an indestructible flag, and a 24-bit colour) and rows of single-character style indices. `render` passes the whole set to the canvas in its `canvas` state, so every viewer plays the same levels. saving sends **one** level over `on_canvas_message` (`save_level`, with its index) to stay well inside the 4 KiB message limit; the server checks the poster, validates the shape (row count and width, known style indices, at most 16 styles, at least one breakable brick) and writes the merged set. `reset_levels` restores the built-in five.

the daily board is stored in UTC date buckets and cleared by a server-scheduled timer at the next UTC midnight. the all-time board is never cleared. scores come from the Canvas Guest, so this example demonstrates leaderboard plumbing rather than tamper-proof anti-cheat.

## Scaffold and bindings

this project was created from the CLI's Canvas scaffold (run from the repository root):

```bash
cargo run -p chittr-cartridge -- new breakout --canvas --sdk-path "$PWD/sdk/chittr-cartridge"
```

the Server Guest exports `init`, `render`, `on_canvas_message` and `on_timer`; the Canvas Guest exports `start`, `frame` and `event`. their `pdk.rs` files are generated from `docs/cartridges/v1/` with `xtp plugin init` and pruned by `chittr-cartridge sync`. to regenerate both bindings:

```bash
SCRATCH=$(mktemp -d)
xtp plugin init --schema-file docs/cartridges/v1/server.xtp.yaml --template Rust --path "$SCRATCH/server" --name breakout -y
chittr-cartridge sync "$SCRATCH/server" examples/breakout
xtp plugin init --schema-file docs/cartridges/v1/canvas.xtp.yaml --template Rust --path "$SCRATCH/canvas" --name breakout-canvas -y
chittr-cartridge sync "$SCRATCH/canvas" examples/breakout/canvas
rm -rf "$SCRATCH"
```

## Build and check

```bash
./build.sh                          # builds the wasm into cartridge/
chittr-cartridge check cartridge    # validates the Cartridge with the server's rules
chittr-cartridge pack cartridge     # writes breakout.cartridge, validated
chittr-cartridge watch cartridge    # repacks and checks on every save
```

upload the `.cartridge` in the Chittr composer, with **Publish as preview** on, to play it on a Chitt.

the guests' logic is tested natively too:

```bash
cargo test --manifest-path examples/breakout/Cargo.toml         # leaderboard state and the daily rollover
cargo test --manifest-path examples/breakout/canvas/Cargo.toml  # level layouts, power-ups and collisions
```

## Files

- `src/lib.rs`: score validation, KV-backed leaderboard state, the daily timer and the Server Guest Hooks.
- `canvas/src/lib.rs`: levels, physics, power-ups, particles, rendering and score submission.
- `src/pdk.rs`, `canvas/src/pdk.rs`: generated ABI bindings.
- `cartridge/`: the manifest, Liquid card template and built wasm files.
