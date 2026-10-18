# Hurtle

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

ok this one is a whole thing. wordle-ish, but each wrong guess costs health and paints the walls. ive been told many
such things. good luck c;
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

Hurtle is a Wordle-style Chittr Cartridge. everyone gets the same hidden twelve-letter word, which changes every 24
hours. when signed in, u get one attempt (up to six guesses) per daily word, a colored letter grid, and an on-screen
keyboard that also accepts keyboard input. keyboard letters show gray for absent letters, yellow for present letters
whose position is not confirmed at the current guess index, and green for letters confirmed at that index.

## The chamber

the card and canvas share a dark stone-and-brass execution-chamber theme, with raised tiles, an active-letter cursor,
numbered attempts, and a six-segment **health bar** in a compact 320 × 400 canvas. each player starts each word with
**6 health**. every fresh, valid wrong guess costs one health and leaves **one splatter** in their profile `textColor`.
a correctly formatted word outside the guess list, or a repeated guess, also costs one health and leaves one splatter,
but the row receives no letter marks and the keyboard gains no feedback. both kinds of unscored guesses look the same, so
neither reveals whether the word was in the list or whether its letters match the answer. incorrect-length or non-letter
input is rejected without damage. the sixth damaging guess reduces health to zero and leaves **three death splatters**
instead of one (eight across a full losing attempt). a survivor keeps their remaining health and all the blood they
spilled along the way.

each persistent, deterministic stain is an impact (merged lobes, fingers breaking into drops, a cone of spray and thin
runs), a swung cast-off trail, or occasionally both, chosen by the stain's seed. older stains dry darker. the stains are
shared with all viewers, including logged-out and suspended viewers; a Render Push refreshes other open cards without
clearing anyone's draft or changing their health. the card separately counts splatters and deaths; nonfatal hits never
enter the obituary. a new daily word washes the walls clean and starting it restores full health. blood is decorative
and behind the playing surfaces, so the grid and keys remain legible.

the canvas animates only while something happens: typed tiles pop, keys press, a short or refused guess shakes its row
(a refused guess returns to the board), and each accepted guess's tiles flip one at a time. unscored rows stay neutral:
the entered letters are visible to their player, but no marks or keyboard colours reveal anything about the answer.
keyboard colours and input wait for the flip. health drops and the hit's splatter lands only after the row has turned. a
win bounces the winning row and throws sparks; a loss shakes the room, fades a red edge in, and lands all three death
splatters after the final reveal. any new stain, including another player's, grows in and its runs creep down. with
reduced motion, flips, shakes, bounces, sparks and growing stains are skipped.

the server derives health from saved guesses and snapshots colour and seeds for every damaging submission, including
unscored invalid-word and duplicate guesses. it validates RGB hex (`#RRGGBB` or `#RGB`, also accepted without `#`) and
uses blood red if missing or invalid. the atomic daily ledger deduplicates hits separately from deaths, including retries
and refused penalties. saved wounds allow an interrupted blood write to be recovered without adding another hit or
changing its colour. public blood state includes only colours, layout seeds, and separate splatter/death totals—not IDs,
other players' guesses, or the answer. drawing and template decoration use the latest 64 stains; full totals and retry
deduplication are retained. old ledgers expire after 48 hours.

upgrading to 0.2.9 preserves old stains, attempts, deaths and the rotation timer. existing attempts derive health from
their saved guesses, but old wrong guesses are not retroactively splattered. an unseen legacy death still replays its
original single stain; new deaths replay their three-stain group if the host restarts the canvas mid-animation.

starting the game is an explicit opt-in. if six damaging guesses reduce your health to zero, the Server Guest calls the
existing `suspend_character` Poster Action with a fixed one-hour duration and a Hurtle-specific reason. the failure
reply with the final board is posted before the suspension action in the same Hook, so the loss is reported before the
you lose site access. if you have an existing active suspension, you cannot start or submit guesses: the Server Guest
refuses them and the canvas locks its keyboard. your board stays on screen, and until a canvas has finished showing your
death, any new canvas (such as the one the host may start when the suspension redraws the card) plays it again from the
last row, then reports it shown. that report is the one message the Server Guest accepts from a suspended Character. the
host passes that status to Live Chitt guests without exposing the suspension's reason or end time. the Chittr host
checks the Cartridge poster's `character_suspension` permission on every action. if the poster lacks it, the game ends
and the card reports that the penalty could not be applied; no broader capability or new host API is added.

the server scores a guess only when it is a fresh dictionary word of the word's length, and scores it only against the
current daily word. correctly formatted non-words and repeated guesses consume a turn and health but receive no feedback
marks; bad characters and wrong lengths are rejected before damage. since 0.2.12, these unscored rows do not change
keyboard colours or reveal any letter information. the server keeps the word, your progress, and penalty status in the
Chitt's KV store. a recurring 24-hour timer rotates the answer; a new game becomes available to you after rotation. on a
win or loss, the Server Guest replies to the Live Chitt with a mention, a varied bit of flavor text, and the
guess-pattern breakdown. replies describe wins and losses with Hurtle's executioner/death theme; a loss says the
execution was carried out only when the penalty action succeeds. the answer is revealed only to winners. replacing the
Cartridge runs `migrate`, which keeps the current word, every attempt, the blood and the rotation timer. since 0.2.7,
migration also queues an `edit_chitt` Poster Action to refresh the stored Chitt description from the same text used by
`init`, correcting older descriptions that still say five letters. `init` is not rerun on upgrade; the edit takes effect
after the version swap commits, under the poster's normal edit authority. deferred action failures appear in the host's
Poster Action log. possible answers are listed in `cartridge/assets/answers.txt`. a guess must appear in
`cartridge/assets/guesses-12.txt` (or `guesses-5.txt` for a five-letter game), the twelve- and five-letter words of the
public-domain ENABLE word list. a non-word is refused without using up a guess: the row shakes and the draft stays on
the board to fix.

Hurtle used five-letter words before 0.2.0. a Chitt that was already running when it upgraded keeps its live five-letter
word, drawn from `cartridge/assets/answers-5.txt` in its original order, until the daily timer picks the next word; from
then on every word has twelve letters. a game keeps the length of its own answer, so a five-letter game started before
the switch can still be finished. the board narrows its tiles for long words and keeps the five-letter layout as it was.

## Obituary and survivors

under the board, the card lists today's fallen and today's survivors, newest first, each as their mention with a dot in
the colour they finished in (the colour of a victim's stain). the server keeps one roll per day (`roll:{day}`, expiring
after 48 hours with the other daily records); a loss or win adds the player and refreshes everyone's card. the card
shows the newest 24 names under each heading, in a list that scrolls past six names so the card never grows, and counts
the rest. host style filters strip `<style>`, so the scroll cue is inline: the seventh name peeks out half cut off, the
bottom of the list fades, and "↓ Scroll for more" appears, each only when the server's count says the list will scroll.
each heading shows that day's total. only mentions and colours reach the card, never IDs.

`migrate` rebuilds today's roll from what was recorded before the roll existed: the fallen from the blood ledger and the
survivors from their win announcements. those records hold no names, so each such player is counted as "not yet known"
until they next view the card, when their mention fills in.

## First-guess badge

the admin section has a **First-guess win badge** form. enter an existing **badge ID**, not its name, and save; saving
an empty field disables awards. IDs are trimmed and limited to 128 ASCII letters, digits, hyphens or underscores. the
saved ID is displayed separately because the host strips an input's `value` attribute. only site admins can see or
change this setting; the server checks the trusted `Observer.isAdmin` before accessing storage. the setting is stored
under `first-guess-badge`, survives ordinary daily rotation and upgrades, and is removed by **Clear all data**. it cannot
be changed during reset cleanup.

when a player wins with their **first valid guess**, the server snapshots the configured badge ID into the completed game
and calls `grant_badge_to_character` for that player's trusted Observer ID. invalid/non-word submissions do not count as
guesses. wins in two or more guesses and losses never award it. no badge is awarded if the setting is disabled; there is
no retroactive award for old wins. changing the setting affects future wins, not a reward already saved with a winning
game. the Chitt's **poster** must hold `grant_badge` permission; an admin configuring the setting does not lend the
poster that permission. badge existence is checked by the host when the grant runs, not by the setting form.

the game records an atomic claim and its outcome, so replayed winning messages cannot grant twice. if the Observer
already has the badge equipped, the reward is satisfied without making another grant call; otherwise the host's grant
operation is idempotent, including for an already-owned but unequipped badge. grants add the badge to the Character's
available badges but do not equip it automatically. to display it next to the trolltag, the winner can equip it in
**Settings → Badges**. the winner's card confirms the reward or explains a refused grant without undoing their win or
blocking their winning reply. a one-guess win uses its own personalized congratulations rather than the ordinary win
message. the thread reply says **"Check your badges for your reward!"** only when the first-guess reward is confirmed
(including when the player already holds it). if the host explicitly refuses a grant, the reply says it was not assigned
and includes the refusal reason; disabled rewards and uncertain outcomes never claim success. explicit refusals can be
retried by resubmitting the completed winning guess after fixing the badge ID, permission or rate limit. an ambiguous
transport failure stays pending and is not retried automatically; check the Character's badges and the Poster Action log
first. reward state stays with the game, not in a short-lived deduplication key, and old saves without that state still
load normally.

## Admin reset

site admins see **Clear all data & start next day** beneath the rolls. it is a one-click, destructive action, without a
confirmation dialog. both visibility and server authorization use the host's trusted `Observer.isAdmin`; being the
poster alone does not grant access, and a forged Control from a non-admin is refused before touching storage.

the reset deletes **all keys in this Live Chitt's KV store**, including attempts, blood, rolls, announcement and penalty
markers, and unknown/legacy keys. it advances the current puzzle by one (not back to the Chitt's creation date),
recreates only the fresh day/word-length/timer metadata, refreshes everyone's card, and schedules the following word
24 hours later. the new word uses twelve letters, including when resetting a legacy five-letter day. players must opt in
again. posted replies and site suspensions are not KV data and are **not** removed or reversed.

the host has no bulk-delete call and caps Controls at 500 host calls and one second. up to 128 old keys are deleted in
the click. larger stores automatically continue in 128-key batches on a 10-second timer, including beyond the
1,000-key listing cap. during cleanup the card shows a status message, play is blocked, and the admin button becomes
**Resume cleanup**. the next word opens as soon as deletion finishes; large stores cannot be cleared instantly with this
ABI. a durable reset marker preserves the intended day across interruptions, and retries resume that reset rather than
advancing again. a short worker lease prevents simultaneous cleanup workers. replayed old daily timers cannot advance
the new word again or move its deadline. normal version migration still preserves play; it resumes a pending admin reset
if there is one.

## Build and test

Hurtle is live, so every build that will be uploaded bumps `version` in `cartridge/manifest.json`.

from the `chittr-cartridge` repository root:

```bash
cargo test --manifest-path examples/hurtle/Cargo.toml
cargo test --manifest-path examples/hurtle/canvas/Cargo.toml
./examples/hurtle/build.sh
chittr-cartridge check examples/hurtle/cartridge
chittr-cartridge pack examples/hurtle/cartridge
```

upload the generated `examples/hurtle/hurtle.cartridge` through the Chittr composer. a poster without
`character_suspension` can still publish and run the game, but failed guesses will show that the penalty was refused.

## Files

- `src/lib.rs`: per-player game state, scoring, the shared blood ledger, and the guarded suspension action.
- `src/badges.rs`: the admin badge setting and idempotent first-guess reward.
- `canvas/src/lib.rs`: chamber drawing, health bar, seeded impact and cast-off stains, the effect timeline, the board, virtual
  keyboard, and keyboard/pointer input.
- `cartridge/manifest.json`: the opt-in start, admin-only reset, and badge-setting Controls.
- `cartridge/templates/card.liquid`: game instructions, the explicit penalty warning, daily rolls, reward status, and admin controls.
- `cartridge/assets/`: the twelve-letter answers, the five-letter answers live Chitts finish their day on, and the
  valid guesses for each length.
