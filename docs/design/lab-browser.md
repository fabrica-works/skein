# The lab's browser

Provisional, 2026-10-22. The lab's one program over skein's browser kit
(browser.md), and the look's capture that drives it: a product's pages
captured beside their mockups, as three views a check and a judge read.
temper's draft `docs/design/draft/qa-web.md` is its first user: its
scenes (section 3 there), their three views (section 4) and their
judging (section 5). The lab's records, runner and checks are
`lab.md`'s; this document says what the browser adds to them. The hand,
a driver's browser in a cluster (qa-web.md, section 6), is this same
program grown, and is not designed here (section 8).

## 1. In one page

- **One program on the kit:** `skein-lab-browser`, the lab's browser.
  It runs headless Chromium as its child through `skein-browser`, from
  one loop, and answers calls. The capture uses it now, and the hand
  later, so that both find what they find as a test's person does: by
  role and accessible name, with the kit's bounds and its trouble
  (section 2).
- **Calls, not scripts:** one canonical JSON line in, one out, on its
  standard input and output. A call opens a page at a size, goes to an
  address, awaits a part, reads the page's tree, a part's style or a
  shot. Nothing evaluates script in a page (section 3).
- **The look's reads are the program's alone.** Its style, tree,
  selector, shot and emulation calls are the kit's look reads
  (browser.md, 5.1), which no test sends: a test sees what a person sees.
- **It serves what it captures:** each directory a call names, read
  whole, on a loopback origin of its own, so that each side keeps its
  origin's rules (2.3).
- **A closed network:** Chromium reaches the program's own origins and
  nothing else. Every other request goes to a proxy the program holds,
  which refuses it and records what was asked (2.4).
- **The lab's fonts:** Chromium sees only the fonts of one directory
  the lab names, through a font configuration of its own, so that a
  shot depends on files the key names, not on the machine (2.5).
- **The capture** is Python's standard library in `scripts/lab/look/`,
  under the runner's "for now" (lab.md, 10.1): a scene file in, the
  program's calls, a record and three views out (sections 4 and 5).
- **Three views of each scene at each width:** its structure, an
  outline of each side's tree, diffed as text; its style, a table of
  the paired regions' computed values and boxes; its picture, the two
  shots side by side, each region cropped from both at twice the scale,
  and a difference image with its PSNR where a region's content matches
  (5.3).
- **Tested where each part lives:** the kit's reads in the kit's own
  tests, the program by one story on real Chromium in skein's browser
  suite, the capture's decisions in the runner's unit tier and its
  whole run in the runner's process tier (section 6).

## 2. The program

### 2.1 Its place and its rules

- **`testing/skein-lab-browser`,** a binary crate in skein's workspace,
  named in no product's dependencies. It builds on `skein-browser`, and
  on `skein-fake-person-browser` for its origins' server (ui-web.md,
  14.2) and, with the hand, its face.
- **Ordinary Rust,** as the face's harness is (ui-web.md, 14.2): one
  thread, one loop, skein's real loop under it, Chromium and the
  origins its children and listeners; the test crates' rules
  (`clippy.toml`) and no others.
- **Generic:** it names no product, no page and no path. What it opens
  and reads is its caller's.

### 2.2 Its command line and its life

```
skein-lab-browser --chromium PATH --fonts DIR --scratch DIR --out DIR
```

- **`--chromium`:** the browser's program, by absolute path. The lab
  names one version, by path and digest (section 7).
- **`--fonts`:** the fonts' directory (2.5).
- **`--scratch`:** an empty directory of the run's: Chromium's profile
  (browser.md, section 3) and the font configuration's cache. Nothing
  in it is evidence.
- **`--out`:** where shots are written, each by a name relative to it
  that climbs nowhere.
- **Its life:** it binds its proxy (2.4), spawns Chromium with the
  kit's command, the proxy's and fonts' arguments and environment
  added, and once the kit is ready prints `{"ready":{"browser":V}}`,
  V being the version the kit reports. Then it reads calls, one at a
  time: it answers each before it reads the next. The end of its
  standard input closes every page, then the browser, and it exits 0
  once Chromium's exit is confirmed. A browser that ends by itself, or
  a call it cannot answer, ends it with status 1 and a line on its
  standard error.
- **Its limits** are the kit's (browser.md, section 8), its origins'
  (2.3) and a call's line, each fixed at start and stated on its
  standard error when a call passes one.

### 2.3 Origins

- **`serve` reads a directory whole:** its regular files, under the
  limits of a file's and the origin's bytes and their count, refused
  past any, and links and special files refused. It answers every
  `GET` of a file's path with the file and a type by its extension, and
  anything else not found.
- **One origin each:** each `serve` binds a listener of its own on
  `127.0.0.1`, port 0, so that a page's icons, masks and fonts are
  fetched under their own origin's rules, as a product's bundle serves
  them. The origin's address is its answer.
- **Only served origins are opened:** `go` refuses an address on none
  of them.

### 2.4 The closed network

- **The proxy:** a listener the program binds on `127.0.0.1`, port 0,
  before Chromium starts, named in Chromium's arguments
  (`--proxy-server`). Chromium sends it every request but those to a
  loopback address, which go direct (its implicit bypass): so a page
  reaches its own origins and no other host, whether named or by
  address.
- **Refused and recorded:** the proxy reads a request's first line and
  answers `403`, closing the connection, and keeps the host and port
  asked. `trouble` reports each as `network`, beside the page's own
  (browser.md, section 7), so a page that wants the network says so;
  Chromium's failed load shows there too, as a log error. The proxy
  cannot tell pages apart, so a `trouble` reports every refusal since
  the last, whichever page asked: the capture keeps one page open at a
  time.
- **No resolver:** Chromium's host resolver maps every name to none
  (`--host-resolver-rules`), so nothing is looked up.

### 2.5 Fonts

- **A font configuration of its own:** the program writes
  `fonts.conf` in its scratch directory, naming the fonts' directory as
  the one font directory and the scratch directory as fontconfig's
  cache, and its rules for the families a page or the browser names but
  the directory lacks (the generic ones, and the browser's defaults such
  as Arial and Times New Roman) from the directory's own
  `aliases.conf`, when it has one. It gives
  Chromium `FONTCONFIG_FILE` naming it, beside the kit's environment
  (browser.md, section 3).
- **So a page's fonts are files:** a page's web fonts, from its own
  origin, and the directory's for every family a page names without
  one. The machine's installed fonts are not seen.
- **The directory is the lab's,** named in its settings and digested in
  its key (section 7). Its first holds the DejaVu families (Sans, Sans
  Mono, Serif) and their aliases.
- **A font from another host is not drawn.** A page that loads its fonts
  from elsewhere draws with the directory's in their place, and its
  trouble names the request refused (2.4). The capture records both;
  how a product's scenes treat it is theirs.

## 3. The calls

A call is `{"call": NAME, ...}` on one line; its answer one line, a
member named for its result, or `{"refused": WHY, "detail": TEXT}`.
Lines are canonical JSON (lab.md, 2.2). A page is named by the caller,
a node by the kit (browser.md, section 5).

| Call | Its members | Its answer |
|---|---|---|
| `serve` | `origin`, a name; `dir`, an absolute path | `{"served":{"origin","address"}}` |
| `open` | `page`, a name; `width`, `height`, `mobile` | `{"opened":PAGE}`: a new person and page at `about:blank`, its viewport at scale 1, light scheme, reduced motion |
| `go` | `page`; `url`, on a served origin | `{"done":PAGE}`, once the load event fires |
| `await` | `page`; `role`, `name`, `match` (`exact` or `words`); `seconds` | `{"met":[SEEN]}` or `{"missed":[SEEN]}` |
| `find` | `page`; `role`, `name`, `match` | `{"found":[SEEN],"more":N}` |
| `tree` | `page` | `{"tree":[{"node","depth","role","name","level"}]}`, the accessibility tree in document order, ignored nodes left out, `level` a heading's or a tree item's, or 0 |
| `select` | `page`; `selector`, CSS | `{"selected":NODE}` or `{"selected":null}` |
| `style` | `page`; `node`; `properties` | `{"style":{"values":{NAME:VALUE},"faces":[{"family","custom","glyphs"}],"box":RECT}}` |
| `shot` | `page`; `clip`, a `RECT` or null for the viewport; `scale`, 1 or 2; `file` | `{"shot":{"file","bytes","sha256","width","height"}}` |
| `trouble` | `page` | `{"trouble":[{"kind","text"}]}`, what the page reported since the last |
| `close` | `page` | `{"closed":PAGE}` |

- **`SEEN`** is `{"node","role","name","box"}`, and **`RECT`**
  `{"x","y","width","height"}`, in whole CSS pixels of the document.
- **`style`'s values** are Chromium's computed values, as strings, for
  the properties named; `faces`, the faces that drew the node's own
  text, by family, whether a web font (`custom`), with their glyphs'
  count; `box`, the node's border box.
- **`trouble`'s kinds** are the kit's (`exception`, `console`, `log`,
  `crash`, `dialog`) and the proxy's `network`.
- **A refusal** names the kit's refusal (`gone`, `hidden`, `limit`,
  `timeout`, `protocol`, `crashed`) or the program's: `page` (no such
  page), `origin` (an address on no served origin), `call` (a call it
  does not know, or a member missing, unknown or of the wrong type).

## 4. The scene file

`look-scene/1`, canonical JSON, one file a scene, kept by the product
beside its look experiment. The capture reads it; the look's checks
read it too, with the members they add (`expectations`, `departures`).

```json
{"format": "look-scene/1",
 "name": "chat",
 "widths": [{"name": "desktop", "width": 1440, "height": 900, "mobile": false},
            {"name": "phone", "width": 390, "height": 844, "mobile": true}],
 "mockup": {"path": "/chat.html?world=solo&task=T10&bare",
            "ready": {"role": "main", "name": "", "match": "words"}},
 "page": {"path": "/chat-t10.html",
          "ready": {"role": "main", "name": "", "match": "words"}},
 "regions": [
   {"name": "banner", "mockup": {"role": "banner", "name": "", "match": "words"},
                      "page": {"role": "banner", "name": "", "match": "words"}},
   {"name": "composer", "mockup": {"selector": "form textarea"},
                        "page": {"role": "textbox", "name": "Message", "match": "exact"}}],
 "names": ["Chats", "Inbox", "Accounts", "Send"],
 "expectations": [],
 "departures": []}
```

- **`name`** matches `[a-z][a-z0-9-]*`, and names the scene's files.
- **`widths`:** each a viewport, `mobile` as a phone's.
- **`mockup` and `page`:** each side's path on its origin, and its
  readiness, a part awaited; `mockup` is null for a page no mockup
  draws, which is captured alone.
- **`regions`:** pairs of a part found on each side, by role and name,
  or on the mockup's side by a CSS selector where its part has no role
  of its own. The page's side is found by role and name only, as a
  person finds it. An empty name by words matches any name; a finder
  that matches several takes the first in document order, and the
  record counts the rest.
- **`names`:** the words the structure shows: the frame's and the
  controls', never content.
- **`expectations` and `departures`:** the look's checks' (qa-web.md,
  5.3), which the capture does not read.
- **Refused,** each problem named, before the browser starts: another
  format, a missing or unknown member, a name that repeats, a finder
  with both a role and a selector or neither, a selector on the page's
  side, a path not starting with `/`.

## 5. The capture

### 5.1 Its run

```
scripts/lab/look/capture --browser PROGRAM --chromium PATH --fonts DIR
    --mockups DIR --page DIR --ready-seconds N --out DIR SCENE...
```

- **The program its child,** started with the paths given, its scratch
  a temporary directory the capture removes at its end; the two
  directories served, `mockups` and `page`, each its own origin.
- **For each scene, each width, each side:** `open` at the width;
  `go` to the side's path on its origin; `await` its readiness within
  `--ready-seconds`; then `tree`; each region found (`find` or
  `select`), its `style` with the properties of 5.3, its `shot` at scale
  2 clipped to its box; the page's `shot` at scale 1; `trouble`;
  `close`. A side that misses its readiness, or a region not found, is
  recorded so, and the rest goes on.
- **Exits** 0 once every scene is recorded, whatever it shows, 2 on a
  scene refused (section 4), 1 when the program fails.

### 5.2 The record

In `--out`, beside the files it names:

```
capture.json                          look-capture/1
<scene>/<width>/<side>/tree.json      the tree call's answer
<scene>/<width>/<side>/outline.txt    the structure (5.3)
<scene>/<width>/<side>/page.png       the viewport, scale 1
<scene>/<width>/<side>/<region>.png   a region, scale 2
<scene>/<width>/structure.diff        the outlines' difference
<scene>/<width>/style.md              the style table
<scene>/<width>/pair.png              the page shots side by side
<scene>/<width>/pairs/<region>.png    a region's crops side by side
<scene>/<width>/diff/<region>.png     a region's difference image
```

`capture.json` is `{"format": "look-capture/1", "browser", "scenes"}`:
each scene its file's digest and, per width, each side's address, its
readiness (`met` or `missed`), its trouble, its files, and per region
whether it was found, its box, its values and faces, and, where its
content matches, its PSNR in hundredths of a decibel; every file named
by its path, size and digest (lab.md, 2.2). `<side>` is `mockup` or
`page`.

### 5.3 The views

- **Structure:** `outline.txt`, from the tree: the landmarks (banner,
  navigation, main, complementary, contentinfo, a named region, form,
  search), the headings with their levels, the lists with their items'
  count, and the controls by role (button, link, textbox, checkbox,
  radio, combobox, tab, menuitem, switch), in document order, indented
  by the landmarks they are in. An entry shows its name when the name
  is one of the scene's `names`, and its role alone otherwise.
  `structure.diff` is the two outlines' unified difference.
- **Style:** per region pair, the computed `font-family`, `font-size`,
  `font-weight`, `line-height` and `letter-spacing`; `color` and
  `background-color`; each side's `border-*-width`, `-style` and
  `-color`, each corner's `border-*-radius`, and `box-shadow`; each
  side's `padding-*`, `row-gap` and `column-gap`; the used faces; and
  the box's `x`, `y`, `width` and `height`. `style.md` is the table of
  both sides, a difference marked by the default rule: colours, faces
  and keywords exact, lengths within one pixel.
- **Picture:** `pair.png`, the two page shots side by side; for each
  region, its two crops side by side; and, where the region's content
  matches (the names of every node under it equal on both sides), the
  difference image, the two crops placed at their top left on the larger
  of their sizes, the area one does not cover counted as differing, and
  its PSNR. A region found on one side only has its one crop.

## 6. Tests

- **The kit's reads** are the kit's (browser.md, 9 and 5.1): its step
  tests, its machine world, and its facts on real Chromium.
- **The program:** one story in skein's browser suite (browser.md,
  section 9): the built program, with the suite's Chromium
  (`SKEIN_TEST_CHROMIUM`) and a fonts' directory named as it is
  (`SKEIN_TEST_FONTS`, else Debian's `fonts-dejavu-core` directory,
  `/usr/share/fonts/truetype/dejavu`; neither holding DejaVu fails the
  suite at its start, naming both and the package), a fixture directory served, a page
  opened at the phone's size, a part awaited, its tree, a selector's
  node, a style, a clipped shot at scale 2, a page whose external
  stylesheet the proxy refuses (its `network` trouble, and the page
  drawn without it), a call refused, and the end of its input closing
  everything.
- **The capture's decisions** in the runner's unit tier (lab.md, 10.3),
  pure: the scene file and its refusals; the outline from a tree; the
  style table and its rule; PNG's reading and writing, the side by
  side, the difference image and the PSNR on small images; the record's
  canonical form.
- **The capture's run** in the runner's process tier: the built program
  and the lab's Chromium on a fixture scene whose page side moves one
  region and changes one colour, its record showing both. A missing
  program or Chromium fails the story, naming what is missing.

## 7. In the lab

The look experiment and the settings that name the lab's browser, its
Chromium and its fonts are designed with the look's checks (qa-web.md,
section 5); until then the capture runs by hand. What is settled: the
program, Chromium (with its version) and the fonts' directory are the
lab's, named in its settings and digested in its key (lab.md, 2.3), as
the driver's host is.

## 8. Open questions

- **The hand:** the program's calls on a socket in its directory, the
  person's acts through the face, and the sign-in (qa-web.md, section
  6).
- **A dark scheme** for a product whose look follows the system's.
- **Pages that animate** past reduced motion: readiness by a part
  shown holds for a static page; a page that moves after it would need
  an expectation of its own.
