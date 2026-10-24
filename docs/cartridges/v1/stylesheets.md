# Stylesheets and badges

-----BEGIN PGP SIGNED MESSAGE-----
Hash: SHA512

ok so ur card can wear its own clothes now c; drop a style.css next to ur manifest, use tailwind classes in ur
templates, and none of it leaks onto the rest of the site or anyone else's card. and if ur on v2 u get badges 2.
lemme know how it feels
-----BEGIN PGP SIGNATURE-----

iQEzBAEBCgAdFiEEc2hpdHR5IGZha2Ugc2lnbmF0dXJlIGJ1dCBpdHMgdmliZXM=
-----END PGP SIGNATURE-----

this page covers the stylesheet a Cartridge can carry, the `class` attribute in templates, and the two badge Host Parts.
the stylesheet and `class` work in **v1 and v2**. the badge elements are **v2 only**: the rules for them live in the
[v2 ABI](../v2/abi.md#11-badge-host-parts), and this page walks thru them too.

## The short version

```text
my-cartridge/
  manifest.json
  server.wasm
  style.css            <- new, optional
  templates/card.liquid
  assets/img/paper.png
```

```css
/* style.css */
.note {
  @apply rounded-box p-4 text-base-content;
  background: url(img/paper.png);
}
```

```html
<!-- templates/card.liquid -->
<section class="note">
  <lc-author></lc-author>
  <p class="text-lg font-bold text-primary">{{ question }}</p>
</section>
```

that's it. no manifest key, nothing to register. if `style.css` is in the archive root, the host picks it up.

## Where it goes

a Cartridge may carry one stylesheet, `style.css`, at the archive root next to `manifest.json`. it's found by its path,
the same way `server.wasm` and `canvas.wasm` are, so there's no manifest key for it. it has to be valid UTF-8 and at most
64 KiB. one file is plenty, since Tailwind's `@apply` and `@utility` cover most reasons ppl split CSS.

`class` is allowed on any element in a template. a class does something only inside ur own card.

## How scoping works

ur card is drawn inside its own shadow root. the host adopts the site's CSS there first, then ur compiled `style.css`. so:

- ur rules reach **only ur card**. not the page, not the chitt next to u, not the author row outside the card. a rule
  for `body`, `:root` or `*` stays inside the card too.
- the Host Parts (`lc-author`, the menu, reactions, ...) still look like the rest of the site, and so do the daisyUI and
  Tailwind classes the site itself uses.
- the card's box sits outside the shadow root, so `position: fixed` (and `.fixed`) can't escape the card. it's fixed to
  the card, not the screen.
- while ur stylesheet loads, the card stays hidden so it doesn't flash unstyled. if it fails to load, the card draws with
  the site's CSS alone.

## Tailwind

the host compiles ur stylesheet with Tailwind v4 once, when u publish or replace the Cartridge. it scans every token in
ur templates and in `style.css`, and emits the utilities u actually use. ur own CSS goes in as Tailwind input, so these
work:

| Directive | What it's for |
| --- | --- |
| `@apply` | use utilities inside ur own rules |
| `@theme` | add or change theme tokens (`--color-mint: #0f8;` makes `bg-mint`) |
| `@utility` | define a custom utility |
| `@variant`, `@custom-variant` | use or define variants |

a few things to know:

- **theme**: utilities read the page's variable first and fall back to Tailwind's default, like
  `var(--color-red-500, oklch(...))`. so ur card follows the site's theme and the viewer's Skin for free.
- **daisyUI colours**: `primary`, `primary-content`, `secondary`, `accent`, `neutral`, `base-100`, `base-200`,
  `base-300`, `base-content`, `info`, `success`, `warning`, `error` (and their `-content` pairs) are theme tokens, so
  `bg-primary`, `text-base-content` and friends compile to the page's daisyUI colours.
- **daisyUI components** (`btn`, `card`, `badge`, ...) aren't compiled per Cartridge. they come from the site CSS the card
  adopts, so a component class works only if the site uses it too. if u want one for sure, build it with `@apply`.
- **no preflight**: the site CSS the card adopts already has it.
- **frozen per version**: the compiled CSS is stored with the Cartridge Version. a later Tailwind upgrade on Chittr
  doesn't restyle a version u already published.

classes built from Liquid at render time (`class="bg-{{ colour }}-500"`) only work if the full class name also appears
somewhere Tailwind can see it at upload: in a template, or in `style.css` (a comment works). Tailwind reads tokens, it
doesn't run ur Liquid.

## What url() can point at

a `url()` in `style.css` can name exactly two kinds of thing:

| Form | Example | Becomes |
| --- | --- | --- |
| one of this Cartridge's assets | `img/a.png`, `./img/a.png`, `assets/img/a.png`, `/lc/{sha}/assets/img/a.png` | `url("/lc/{sha}/assets/img/a.png")` |
| an https URL on a subdomain of `chittr.ing` | `https://cdn.chittr.ing/x.png` | the same URL |

the asset has to exist in this version's `assets/`. everything else is refused: `data:` URLs, `http:`, URLs with a port
or credentials, paths that climb (`../`), and lookalike hosts like `chittr.ing.evil.example` or `evilchittr.ing`. the
bare `chittr.ing` (no subdomain) isn't allowed either.

Tailwind's arbitrary values (`bg-[url(...)]`) come from template text, not from `style.css`. they're held to the same
rules after compiling, but a bad one is dropped quietly instead of refusing the upload, so check ur card if a background
goes missing.

## What's refused

these refuse the upload with a `style.css: ...` error that says what broke:

| Not allowed | Why, or what to do instead |
| --- | --- |
| `@import`, `@reference` | can't load other stylesheets |
| `@font-face` | declare fonts in `manifest.json`'s `fonts` (see [Fonts](#fonts)) |
| `@property` | it applies to the whole page |
| `@plugin`, `@config` | no Tailwind plugins or configs |
| `@source` | classes are read from ur templates |
| `@tailwind` | not needed; utilities are always there |
| `@page` and any other at-rule not listed below | not available in a card |
| `image-set()`, `src()` | use `url()` with an asset path |
| backslash escapes, `expression()`, `javascript:` | not available |
| the `behavior` and `-moz-binding` properties | not available |
| a `url()` outside the rules above | see [What url() can point at](#what-url-can-point-at) |
| CSS that doesn't parse | fix the syntax; the error says where |

at-rules u **can** write: `@media`, `@supports`, `@layer`, `@keyframes`, `@container`, `@starting-style`, plus Tailwind's
`@apply`, `@theme`, `@utility`, `@variant` and `@custom-variant`. comments are stripped from the output.

## Fonts

fonts stay in the manifest. name a manifest font by its manifest name in `font-family`, and the host rewrites it to the
family it actually loads:

```json
{ "v": 1, "name": "Zine", "version": "1.0.0", "fonts": { "Pixel": "assets/fonts/pixel.woff2" } }
```

```css
.title { font-family: Pixel, monospace; }
```

`@font-face` in `style.css` is refused. browsers ignore it inside a shadow root anyway, so this way u get one place for
fonts and it actually works.

## Limits

| Limit | Value | Past it |
| --- | --- | --- |
| `style.css` | 64 KiB, valid UTF-8 | refused at upload |
| Compiled CSS | 256 KiB | refused at upload |
| Class candidates scanned from templates and `style.css` | 10,000 | later tokens aren't scanned |

the [limits](limits.md) page has these too.

## Where errors show up

| Where | What it checks |
| --- | --- |
| `chittr-cartridge check` and `pack` | that `style.css` is at most 64 KiB and valid UTF-8. it prints a note when a Cartridge has a stylesheet, since the rest happens on Chittr |
| the composer, when u attach the archive | everything in [What's refused](#whats-refused), before anything uploads |
| the Cartridge Simulator | the full compile, on every reload, the same way the upload does it |
| publish and replace | the full compile; the error is `style.css: ...` |

so the loop is: write CSS, reload the Simulator, look at the card. `check` won't catch a bad `url()`, the Simulator will.

## Badges (v2)

two Host Parts draw a Chittr badge by its id. both are **v2 only**: a `v: 1` template that uses either is refused with
`templates/card.liquid: <lc-badge> and <lc-badge-tooltip> are only allowed when "v" is 2`.

### `<lc-badge>`

```html
<button type="button" data-lc-control="cheer" data-lc-value="go"><lc-badge id="founder" width="16" height="16"></lc-badge> cheer</button>
```

a plain image of the badge. it acts like an `<img>`, so it goes anywhere an image does, including inside a Control's
button, a link or a table cell. a click on it is a click on whatever it's inside.

- `id` is required: the badge's id, 1 to 64 characters of `A-Z a-z 0-9 _ -`. it can come from Liquid
  (`<lc-badge id="{{ badge }}">`), so the CLI doesn't check it.
- it keeps an `<img>`'s attributes: `alt`, `width`, `height`, `class`, `title`, `aria-label`, `dir`, `lang`, and an
  inline `style`.
- `alt` and `title` default to the badge's name, and the size defaults to 24×24, same as on profiles.
- an id that doesn't match any badge draws nothing. so does a badge lookup that fails; the rest of the card still draws.
- it's a leaf: anything u put inside it is dropped.

### `<lc-badge-tooltip>`

```html
<p>earned: <lc-badge-tooltip id="founder"></lc-badge-tooltip></p>
```

the badge the way profiles show it: the image, plus the site's tooltip with the badge's name and description. it takes
the same `id` and attributes as `<lc-badge>`.

the tooltip trigger is itself interactive, so **don't put it inside a button**. use `<lc-badge>` there. the tooltip opens
on hover and focus.

### Which badges

any badge that exists can be drawn, whether or not anyone holds it. badge names, descriptions and images are already
public on profiles. a card can show at most **32 distinct badges**; further distinct ids draw nothing, and repeating an id
u already used is fine.
