# Cartridge limits (ABI v1)

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

the numbers. every limit u'll bump into, what it is, and what happens when u bump it. ive been told many such things, so
these are the ones the server actually uses. if one of them ever disagrees with what u see, the server wins and lemme know
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

this page is the reference. the [ABI](abi.md) explains what the limits are for, the [guide](guide.md) shows where they
come up in code, and the [cookbook](cookbook.md) shows how to work inside them. the CLI's `check` enforces the upload
limits too, so a Cartridge that passes `chittr-cartridge check` won't be refused for them.

what happens past a limit is one of these:

- **refused at upload**: the archive doesn't validate, and the upload is refused with the rule that broke.
- **an error answer**: the call fails with the code shown, and the Hook can handle it. a `rate_limited` is the usual one.
- **a crash or a timeout**: the guest is discarded, and the Hook gets no answer.
- **a clamp**: the host lowers the value to its cap and carries on (the Server Guest's memory).

## Archive and Cartridge files

| Limit | Value | Past it |
| --- | --- | --- |
| Cartridge archive | 32 MiB | refused at upload |
| Inflated size of the archive's files | 64 MiB | refused at upload |
| Files in the archive | 1,000 | refused at upload |
| `server.wasm` and `canvas.wasm` each | 4 MiB | refused at upload |
| An asset file under `assets/` | 16 MiB | refused at upload |
| Template files under `templates/` | 64 | refused at upload |
| One template file | 256 KiB | refused at upload |
| `manifest.json` | 16 KiB | refused at upload |
| `name` in the manifest | 80 characters | refused at upload |
| `version` in the manifest | 40 characters | refused at upload |
| A template's name (the part before `.liquid`) | 64 characters, `A-Z a-z 0-9 _ -` | refused at upload |

a Cartridge Version's `version` string is the author's label. the archive's sha256 is what identifies it.

## Controls

| Limit | Value | Past it |
| --- | --- | --- |
| Controls per Cartridge | 32 | refused at upload |
| Control name | lowercase letter first, then lowercase letters, digits, `-` or `_`; at most 32 characters | refused at upload |
| Values in a `value` enum | 100 | refused at upload |
| Characters in one enum value | 1 to 64 | refused at upload |
| Fields in a `fields` Control | 1 to 16 | refused at upload |
| Characters in a field's name | 32 | refused at upload |
| `maxLength` of a string field | 1 to 2,000 (2,000 is also the default) | refused at upload |
| Uses per Observer per Chitt | 10 in any 10 seconds | "slow down", and no guest runs |
| Uses per Character across Chitts | 60 in any 60 seconds | "slow down", and no guest runs |

## Memory

| Limit | Value | Past it |
| --- | --- | --- |
| Server Guest linear memory | 32 MiB (512 pages) | clamped to 32 MiB when the module loads |
| Canvas Guest declared memory maximum | 64 MiB (1,024 pages) | refused at upload |
| Canvas Guest memory with no maximum | 64 MiB | the host caps it at instantiation |

## Deadlines

a Hook runs under the deadline of its class. the Hook's own clock starts when the host calls it.

| Class | Deadline | Hooks |
| --- | --- | --- |
| lifecycle | 10 s | `init`, `migrate` |
| render | 500 ms | `render` |
| control | 1 s | `on_control` |
| gate | 1 s | `on_reply`, and `on_reaction` and `on_rechitt` when they add |
| background | 2 s | `on_view`, `on_timer`, `on_canvas_message`, and the removal notifications |

| Limit | Value | Past it |
| --- | --- | --- |
| A render that waits in the queue | 1 s | `busy` |
| A Canvas Guest's ready signal after `start` | 5 s | the canvas crashes (`startup`) |
| A Canvas Guest call (`start`, `frame`, `event`) | 250 ms | the canvas crashes (`timeout`) |
| A `send` answer | 5 s | the guest gets `error` with `timeout` |

a gate that runs past its deadline fails open: the action commits as allowed.

## Host calls

| Limit | Value | Past it |
| --- | --- | --- |
| Host calls per `render` | 100 | `rate_limited` |
| Host calls per any other Hook call | 500 | `rate_limited` |
| Cost of one host call | about 0.5 ms, more inside one Hook call | slower, not refused |

every host function call counts toward the cap, `asset` reads included. the guest's log lines don't.

## Hook returns and sizes

| Limit | Value | Past it |
| --- | --- | --- |
| A Hook's return, as JSON, before Liquid runs | 256 KiB | the call fails with `output_too_large` |
| `render`'s `canvas` state | 16 KiB of JSON | the render fails |
| A `send` message from the canvas | 4 KiB of JSON | `too_large` |
| A `reply` from `on_canvas_message` | 4 KiB of JSON | not delivered |
| A gate's denial message | 200 characters (plain text) | cut to 200 |
| A log line | 512 bytes, cut on a character boundary | cut |
| Log lines kept per call | 50 | the rest are dropped |
| Log lines kept in a failure | 20 | the rest are dropped |

## KV store

| Limit | Value | Past it |
| --- | --- | --- |
| Key size | 1 KiB (UTF-8 bytes) | `key_too_large` |
| Value size | 1 MiB | `value_too_large` |
| Keys per Chitt | 1,000,000 | `quota_exceeded` |
| Bytes per Chitt (key and value, counted together) | 64 MiB | `quota_exceeded` |
| Entries from `list` or `get_all` | 1,000 | cut, and `truncated` is true |
| Keys in one `get_many` | 1,000 | `invalid_argument` |
| Value bytes in one `get_many` | 4 MiB total | `invalid_argument` (read fewer keys) |
| Value bytes in one `get_all` | 4 MiB | cut, and `truncated` is true |
| `ttlSeconds` | 1 to 2,147,483,647 | `invalid_argument` |
| `by` in `incr` | magnitude within 2^53 | `invalid_argument` |

## Timers

| Limit | Value | Past it |
| --- | --- | --- |
| Delay | 10 seconds to 30 days (2,592,000 seconds) | `invalid_argument` |
| Timers outstanding per Chitt | 100 | a new name fails with `quota_exceeded` |
| Timer name | 256 bytes | `invalid_argument` |
| Payload | 16 KiB of JSON | `invalid_argument` |
| Failed deliveries before the timer is dropped | 6 | dropped |
| Claim on a delivery before a crashed worker's claim is requeued | 30 s | requeued |

## Poster Actions

| Limit | Value | Past it |
| --- | --- | --- |
| Actions per Hook call | 5 | `rate_limited` |
| Actions per `render` | 1 | `rate_limited` |
| Actions per Chitt per minute (social and admin together) | 30 | `rate_limited` |
| Causation depth | 3 | `depth_exceeded` |

## Render Pushes

| Limit | Value | Past it |
| --- | --- | --- |
| Distinct targets queued in one Hook call | 20 | `quota_exceeded` (logged in the failure log) |
| Pushes from `render` | none | `not_allowed` |

a push is published only after its Hook call returns ok. a Hook that fails, traps or times out publishes none.

## Canvas messages and tokens

| Limit | Value | Past it |
| --- | --- | --- |
| Messages per signed-in Observer per Chitt | 40 in any 2 seconds | `slow_down` |
| Messages per anonymous session per Chitt | 40 in any 2 seconds | `slow_down` |
| Messages per IP across Chitts (logged-out viewers) | 60 in any 1 second | `slow_down` |
| Messages in flight per canvas | 4 | `busy` |
| Canvas token's life | 24 hours | `stale` (the card is drawn again) |
| Control token's life | 24 hours | the Control is refused |

## Canvas page

| Limit | Value | Past it |
| --- | --- | --- |
| Backing store, per side | 2,048 px | clamped |
| One command buffer | 4 MiB | the canvas crashes (`malformed`) |
| Gradient stops | 64 | see the ABI, section 5 |
| Dash segments | 64 | see the ABI, section 5 |
| A string in a draw op | 64 KiB | see the ABI, section 5 |
| A blit's side | 1 to 2,048 px | see the ABI, section 5 |
| Font size | 1 to 1,024 px (clamped) | clamped |
| Font weight | 100 to 900 (clamped) | clamped |
| Frame rate | 1 to 60 fps; 15 under reduced motion | clamped |
| Images per canvas | 32 | `limit` |
| Sounds per canvas | 32 | `limit` |
| Sounds playing at once | 8 | `busy` |
| Decoded image side | 4,096 px | `too_large` |
| Live canvases per page | 4 | not started by the page |
| Crashes per canvas per page session | 3 | the canvas stays stopped |
| Off-screen before the canvas is terminated | 30 seconds | terminated |

## Site-wide

these belong to the site, not to a Cartridge. they show up as `busy` or a refusal from the page's own requests.

| Limit | Value |
| --- | --- |
| Cards rendered per IP | 120 a minute |
| Render Push stream connections per IP | 30 a minute |
| Canvas crash reports per IP | 30 a minute |
| Observers' views of a Chitt | one impression per Observer per 24 hours (`on_view`) |
