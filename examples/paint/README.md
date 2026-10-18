# Place (example Cartridge)

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

the shared pixel board, r/place style. one pixel every 15 seconds, everybody on the same canvas, and the server remembers it
all. pls be nice to the board
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

a shared 512-by-512 pixel board in the style of r/place. the Server Guest (`src/lib.rs`) stores the board per chitt, enforces
a 15-second cooldown per Character, and pushes changes to every viewer. the Canvas Guest (`canvas/src/lib.rs`) is the board:
tap a pixel to select it (and see who placed it), pick one of the 32 colors of the r/place 2022 palette, and press Place. the
pixel shows at once, and the Place button counts the cooldown down. drag to pan, pinch or use the zoom pill to zoom, and use
the host's expand button for fullscreen. other people's pixels flash as they land.

keys: arrows move the selection a pixel at a time (or pan with nothing selected); W/A/S/D pan; `+`, `-` and `0` zoom; Q and E
change color; Enter or Space places; Backspace clears the selection. Escape belongs to the host (it leaves fullscreen and
hands focus back), so the guest never sees it.

the interface is deliberately quiet: the board on a dark surround, a coordinates pill (with the selected pixel's placer), a
loading pill while tiles arrive, a floating palette bar, and a zoom pill in the bottom right (the host's fullscreen button
takes the top right). all of it is drawn by the Canvas Guest.

## Fullscreen

`<lc-canvas fullscreen>` in `templates/card.liquid` makes the host draw an expand button; only a real click enters
fullscreen. the host letterboxes the canvas to its 3:2 aspect ratio and sends a `resize` with the bigger backing store, so
the logical 960-by-640 layout and the view stay as they were and everything is just sharper. the board is drawn on
device-pixel edges, so a bigger backing store needs nothing else.

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

both guests make their host calls through the SDK (`chittr_cartridge::kv`, `push` and `canvas`), so the Server Guest's tests
run against the SDK's in-memory `TestHost` and the Canvas Guest's tests run natively. the Canvas Guest reads each event's
fields from the generated `CanvasEventInput` as one JSON object, which is how its handlers were written.

## How it stays inside the limits

a 512-by-512 board is 256 KiB of pixels, and the limits that matter are much smaller: 16 KiB of canvas state per render,
4 KiB per message and per reply, and 4 MiB per command buffer. so:

- **tiles.** the board is 256 KV entries (`place:tile:0` to `place:tile:255`), each a 32-by-32 tile as runs (raw when that
  is not shorter), behind a little-endian u32 stamp. a placement compare-and-swaps one tile, so placers in different parts
  of the board never contend.
- **stamps.** each placement that changes something takes one stamp from `kv::incr("place:stamp")`; a tile's stamp never
  goes backwards. `render` sends all 256 stamps and inlines the most recently stamped tiles that fit in 13 KiB, so after a
  placement the tile that just changed is almost always inline.
- **paced loading.** the canvas caches tiles by stamp and fetches the rest with `{ "type": "tiles", "ids": [...] }`: one
  fetch in flight, at most about six a second, nearest the view first, which keeps it well inside the host's 20 messages a
  second. a failed fetch (`slow_down`, `busy`, a timeout) is retried after 0.5 s, doubling to at most 8 s, and a new render
  resets the wait; loading never gives up. tiles still to come show as soft gray, and a pill shows the progress.
- **admins.** a render tells an admin's canvas `"admin": true` (no one else's state has the field), and only then does the
  canvas draw a **No cooldown** switch in the bottom left. with it on, each `place` carries `"skipCooldown": true`; the server
  honours that only when the Observer's `isAdmin` is true, and then neither checks nor starts a cooldown (`wait` is 0). the
  switch starts off on every load, and turns off if a later render stops calling the viewer an admin.
- **cooldown.** `place` claims `place:cooldown:<character>` with `kv::set_if_absent` and a 15-second TTL before it writes,
  so two placements at once can't both get through, and deletes it again when nothing ends up placed. the reply is
  `{ "type": "placed", "stamps": [[tile, stamp]], "wait" }`, `{ "type": "cooldown", "wait" }` or `{ "type": "unchanged" }`,
  and each render tells the viewer how long they have left (`cooldown`).
- **confirmations.** the canvas keeps its own pixel shown over incoming tiles until that tile arrives at the stamp the reply
  named, so nothing flickers back; a refused placement is taken back.
- **pixel history.** `place:who:<tile>` holds a u16 placer index per pixel and the placers as JSON, and drops placers nobody
  points at any more. `info` reads one entry; the canvas asks whenever the selection changes, or the selected pixel does.
- **drawing.** each frame redraws only what changed (the dirty cells, the reticle, flashes, and chrome whose content
  changed), clipped, and encodes only the chrome that touches what it redraws. frames are requested only while something
  needs them (a flash at 60 fps; the cooldown, a toast or loading at 10 fps). cell edges are rounded to whole device pixels, and each region is drawn as raw
  pixels (`blitRgba`) or as same-colored rectangles merged across rows, whichever is smaller, so there are no seams and the
  buffer stays under its cap. a view too detailed to fit is drawn as far as the buffer allows, with a hint to zoom in.

## Troubleshooting server memory errors

if repeated renders eventually report `on_render memory limit exceeded: ... pages requested, 512 allowed`, check the
Chittr host's Extism worker lifecycle. with `@extism/extism` 2.0.0-rc13, worker-backed `call()` does not automatically clear
its temporary input, output and host-reply buffers between calls. repeated full-board reads accumulate until they hit
32 MiB, even though the board itself is only 256 KiB.

in Chittr's `lib/live-chitts/server-guest.ts`, the serialized Hook runner must await `plugin.reset()` while the worker is
idle, before each call, and check for cancellation before invoking the Hook. this clears the temporary buffers without
rebooting the Wasm guest or clearing Extism variables. deploy the host fix; repacking the Cartridge, making the board
smaller, or increasing the memory cap does not fix the accumulation. the existing board's KV data needs no migration.

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
