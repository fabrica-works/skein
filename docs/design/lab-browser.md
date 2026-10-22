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
  limits of a file's and the origin's bytes and their count (8 MiB,
  64 MiB and 4096 files), refused `limit` past any, and links and
  special files refused `origin`; at most 8 origins. It answers every
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
  but `127.0.0.1`, the origins' own address, which the rule would map
  too (`--host-resolver-rules=MAP * ~NOTFOUND, EXCLUDE 127.0.0.1`), so
  nothing is looked up.
- **Loopback goes direct,** whatever its port: a page could reach
  another listener on the machine's loopback, which no lab page names.
  The program does not close it; its caller does, starting it in a
  network namespace of its own, whose loopback holds only the program's
  listeners. The capture does (5.1).
- **Its bounds:** a request's first line is read up to 8 KiB, and a
  `trouble` reports at most 64 refusals, the rest as one `network`
  entry counting them.

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
  the properties named; `faces`, the faces that drew the text within
  the node, its descendants' among them, by family, whether a web font (`custom`), with their glyphs'
  count; `box`, the node's border box.
- **`trouble`'s kinds** are the kit's (`exception`, `console`, `log`,
  `crash`, `dialog`) and the proxy's `network`.
- **A refusal** names the kit's refusal (`gone`, `hidden`, `limit`,
  `timeout`, `protocol`, `crashed`) or the program's: `page` (no such
  page), `origin` (an address on no served origin), `call` (a call it
  does not know, or a member missing, unknown or of the wrong type).

## 4. The scene file

`look-scene/1`, canonical JSON (lab.md, 2.2), one file a scene, kept by
the product beside its look experiment. The capture reads it; the
look's checks read it too, with the members they add (`expectations`,
`departures`). Shown spread out here; the file holds its canonical
bytes.

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

- **`name`** matches `[a-z][a-z0-9-]*`, as every name in the file does,
  and names the scene's files.
- **`widths`:** at least one, each a viewport: its `name`, its `width`
  and `height` in CSS pixels, from 1 to 4096, and `mobile`, a phone's
  handling of the page's viewport tag.
- **`mockup` and `page`:** each side's `path` on its origin, and its
  `ready`, a part awaited, by role and name. `mockup` is null for a page
  no mockup draws, which is captured alone; `page` is never null.
- **A path** starts with `/` but not `//`, and holds printable ASCII
  without spaces: a path on the side's origin, and its query. The
  origin serves the file the path names, its query left to the page's
  script.
- **A finder** is `{"role", "name", "match"}`, `match` being `exact` or
  `words` (section 3's `find`), or, on the mockup's side of a region
  alone, `{"selector"}`, CSS, where the mockup's part has no role of its
  own. The page's side is found by role and name only, as a person
  finds it. An empty name by words matches any name; a finder that
  matches several takes the first in document order, and the record
  counts the rest.
- **`regions`:** pairs of a part found on each side, each named. With
  no mockup, a region's `mockup` is null. `page` is no region's name:
  its crop would be the viewport's shot (5.2).
- **`names`:** the words the structure shows (5.3): the frame's and the
  controls', never content. Each nonempty, none repeated.
- **`expectations` and `departures`:** lists, the look's checks'
  (qa-web.md, 5.3), which the capture does not read.
- **Refused,** every problem named with its member, before the browser
  starts: bytes not canonical, another format, a missing, unknown or
  mistyped member, a name not matching or repeated (a scene's among the
  command's scenes, a width's or a region's in its scene), a finder with
  both a role and a selector or neither, a selector on the page's side
  or in a readiness, another `match`, a path or a size out of its rule,
  a region's mockup finder in a scene without a mockup, or none in one
  with, a region named `page`.

## 5. The capture

### 5.1 Its run

```
scripts/lab/look/capture --browser PROGRAM --chromium PATH --fonts DIR
    [--mockups DIR] --page DIR --ready-seconds N --out DIR SCENE...
```

- **Its arguments:** absolute paths, the program and Chromium
  executable files, the fonts, mockups and page directories; `--mockups`
  given when a scene has a mockup; `--ready-seconds` a positive integer;
  `--out` a directory the capture makes, `0700`, refusing one that
  exists. Every scene is read and checked before anything starts.
- **The program in a namespace of its own,** under bubblewrap (`bwrap`,
  found as the runner finds it, on the default path, and among the
  key's `programs`, lab.md 2.3):
  - a process namespace of its own (`--unshare-pid`), so that every
    process the program starts, Chromium's among them, ends with it;
  - a network namespace of its own (`--unshare-net`), whose loopback
    holds only the program's origins and proxy, so that a page reaches
    no other listener of the machine (2.4);
  - the file system read-only (`--ro-bind / /`), with `/dev`, `/proc`,
    `/tmp` and `/run` of its own, and its scratch and `--out` bound
    writable;
  - its environment `PATH=/usr/bin:/bin` alone, and its end bound to
    the capture's (`--die-with-parent`).

  Its scratch is a temporary directory the capture removes at its end,
  once the program has ended (on a failure, its input closed, and
  killed past its deadline); one it cannot remove fails the capture;
  its `--out` is the capture's; its standard error is kept as
  `browser.log` in `--out`. Its ready line gives the browser's version,
  which the record keeps.
- **The two directories served,** `mockups` (when given) and `page`,
  each its own origin.
- **For each scene, each width, each side, one page at a time,** since
  the proxy cannot tell pages apart (2.4), the mockup's side first:
  `open` at the width; `go` to the side's path on its origin; `await`
  its readiness within `--ready-seconds`; `tree`; for each region, its
  part found (`find`, or `select` on the mockup's side), its `style`
  with the properties of 5.3, and its `shot` at scale 2 clipped to its
  box; the viewport's `shot` at scale 1; `trouble`; `close`. Then the
  width's views (5.3), from the files written.
- **It goes on:** a side that misses its readiness is recorded
  `missed`, and read all the same; a region not found, without a box,
  or whose call is refused (a shot past the program's limit, say) is
  recorded so, with the refusal, and the rest goes on.
- **Each answer is awaited** within `--ready-seconds` and 30 seconds
  more. Past that, at an answer it cannot read, at the program's end,
  or at a refused `serve`, `open` or `close` (with no origin or page,
  nothing after it can be read), the program has failed.
- **Exits** 0 once every scene is recorded, whatever it shows; 2 on a
  refusal (an argument, a scene, an `--out` that exists), before the
  program starts; 1 when the program fails, with its standard error's
  tail on the capture's, and `--out` left as it is, without
  `capture.json`.

### 5.2 The record

In `--out`, beside the files it names:

```
capture.json                          look-capture/1
browser.log                           the program's standard error
<scene>/<width>/<side>/tree.json      the tree call's answer, canonical
<scene>/<width>/<side>/outline.txt    the structure (5.3)
<scene>/<width>/<side>/page.png       the viewport, scale 1
<scene>/<width>/<side>/<region>.png   a region, scale 2
<scene>/<width>/structure.diff        the outlines' difference
<scene>/<width>/style.md              the style table
<scene>/<width>/pair.png              the page shots side by side
<scene>/<width>/pairs/<region>.png    a region's crops side by side
<scene>/<width>/diff/<region>.png     a region's difference image
```

`<side>` is `mockup` or `page`. `capture.json` is canonical JSON
(lab.md, 2.2), its lists in the scenes' order, as the command named
them, and in each scene's own:

```
{"format": "look-capture/1", "browser": VERSION, "scenes": [SCENE]}
SCENE   {"name", "scene": DIGEST, "widths": [WIDTH]}
WIDTH   {"name", "width", "height", "mobile", "mockup": SIDE or null,
         "page": SIDE, "structure": FILE or null, "style": FILE,
         "pair": FILE or null, "regions": [REGION]}
SIDE    {"address", "ready", "trouble", "tree", "outline", "shot"}
REGION  {"name", "mockup": PART or null, "page": PART,
         "pair": FILE or null, "content", "diff": FILE or null,
         "psnr", "differing", "pixels"}
PART    {"found": true, "more", "box", "values", "faces", "shot"}
        or {"found": false} or REFUSED
FILE    {"path", "bytes", "sha256"}
REFUSED {"refused", "detail"}
```

- **A scene's `scene`** is its file's digest, `sha256:` and the hex of
  its bytes.
- **A side's** `address` is the URL it went to; `ready` is `met`,
  `missed`, or the refusal of its `go` or `await`; `trouble` is the
  `trouble` call's list, the proxy's refusals among it; `tree`,
  `outline` and `shot` are files, or the refusal of their call.
- **A part's** `more` is how many others its finder matched; `box` is
  its style's border box (a `RECT`, or null for a part not displayed);
  `values` and `faces` are its style's; `shot` is its crop, null
  without a box, or the refusal.
- **A region's** `content` is `same`, `differs` or `unknown` (5.3);
  `diff`, `psnr`, `differing` and `pixels` are its difference image's,
  null where none is made.
- **Every file** is named by its path from `--out`, `/`-separated, its
  size and its digest (lab.md, 2.2).

### 5.3 The views

- **Structure:** `outline.txt`, from the tree, one entry a line, in
  document order: the landmarks (`banner`, `navigation`, `main`,
  `complementary`, `contentinfo`, `region`, `form`, `search`), the
  headings with their levels (`heading 2`), the lists with their items'
  count, the `listitem` entries one level deeper in the tree
  (`list (4)`), and the controls by role (`button`, `link`, `textbox`,
  `searchbox`, `checkbox`, `radio`, `combobox`, `tab`, `menuitem`,
  `switch`). Each line is indented two spaces for each landmark it is
  in, and ends with its name, quoted as JSON, when the name is one of
  the scene's `names`; otherwise it shows its role alone.
  `structure.diff` is the two outlines' unified difference, three lines
  of context, from `mockup/outline.txt` to `page/outline.txt`, empty
  when they agree; with no mockup, there is none.
- **Style:** per region, these computed properties: `font-family`,
  `font-size`, `font-weight`, `line-height`, `letter-spacing`;
  `color`, `background-color`; each side's `border-*-width`,
  `border-*-style` and `border-*-color`, each corner's
  `border-*-radius`, `box-shadow`; each side's `padding-*`, `row-gap`,
  `column-gap`; then the used faces and the box's `x`, `y`, `width` and
  `height`. `style.md` holds a table per region, a row per property,
  the mockup's value, the page's and a mark, `≠` where they differ by
  the default rule:
  - two values agree when, split into tokens (a number with `px`, or
    any other run of characters outside spaces, commas and
    parentheses), they have as many tokens, each `px` length within one
    pixel of the other's, and every other token equal: so colours,
    keywords and families exactly, lengths within one pixel;
  - the faces agree when their families and `custom` flags, sorted, are
    equal, their glyphs' counts left out; the box's members within one
    pixel;
  - a part missing on one side shows `—` there, and marks nothing.
- **Picture:**
  - `pair.png`: the two viewport shots side by side, the mockup's on
    the left, top-aligned, 16 pixels apart, on white; for each region
    found on both sides, its two crops the same way in
    `pairs/<region>.png`. A region found on one side only has its one
    crop; a scene without a mockup has no pairs.
  - **Content:** a region's content is the names of its node's entry in
    the tree and of the entries under it. It is `same` when the two
    sides' are equal, `differs` when not, and `unknown` when a part was
    not found or its node is not in the tree (an ignored node, such as
    an element no role names).
  - **The difference image,** `diff/<region>.png`, only where the
    content is `same` and both crops were taken: the two crops placed at
    their top left on a canvas of the larger width and the larger
    height; each pixel both cover with equal colours drawn as the
    mockup's in grey, faded to a quarter of its contrast, and each other
    pixel, those only one covers among them, drawn magenta
    (`#ff00ff`). `differing` counts those, of `pixels`, the canvas's.
  - **PSNR,** over the red, green and blue of every pixel of the
    canvas, a pixel one crop does not cover at the largest error, 255:
    `10 log10(255² / MSE)`, in hundredths of a decibel, rounded to the
    nearest; null when no pixel differs.
- **Images** are PNG, read as 8-bit RGB or RGBA without interlace,
  as Chromium writes them, and written as 8-bit RGB.

## 6. Tests

- **The kit's reads** are the kit's (browser.md, 9 and 5.1): its step
  tests, its machine world, and its facts on real Chromium.
- **The program:** one story in skein's browser suite (browser.md,
  section 9): the built program, with the suite's Chromium
  (`SKEIN_TEST_CHROMIUM`) and a fonts' directory named as it is
  (`SKEIN_TEST_FONTS`, else Debian's `fonts-dejavu-core` directory,
  `/usr/share/fonts/truetype/dejavu`; neither holding DejaVu fails the
  suite at its start, naming both and the package), a fixture directory
  served, a page opened at the phone's size, a part awaited, its tree,
  a selector's node, a style, a clipped shot at scale 2, a page whose
  external stylesheet the proxy refuses (its `network` trouble, and the
  page drawn without it), a call refused, and the end of its input
  closing everything.
- **The capture's decisions** in the runner's unit tier (lab.md, 10.3),
  pure: the scene file and each refusal of section 4; the outline from
  a tree, with landmarks nested, levels, a list's items and names shown
  or not; the structure's difference; the style rule from both sides of
  each bound (a length one pixel off and past it, a colour, a keyword,
  faces); the content's three answers; PNG's reading (RGB, RGBA, each
  filter) and writing; the side by side; the difference image and the
  PSNR on small images, equal, one pixel off, and of unequal sizes; the
  record's canonical form.
- **The capture's run** in the runner's process tier: the built program
  (`SKEIN_LAB_BROWSER`), Chromium (`SKEIN_TEST_CHROMIUM`) and the fonts
  (`SKEIN_TEST_FONTS`, else Debian's directory, as the program's story)
  on a fixture scene at one small width whose page side moves one
  region and changes one colour. Its record shows both: the moved
  region's entries in `structure.diff`, its box's `y` and the colour
  marked in `style.md`, the recoloured region's difference image with
  its pixels and PSNR, and the pair. The fixture's page also asks for a
  listener the story holds on the machine's loopback, which sees no
  connection. A missing program, Chromium or bubblewrap fails the
  story, naming what is missing.

## 7. In the lab

- **Named in the lab's settings,** `browser` (lab.md, 2.3): the
  program's path, Chromium's path, digest and version, and the fonts
  directory's path and tree digest. Validation refuses a Chromium or
  fonts directory that differs from them, and the run checks them
  again before it starts.
- **Digested in the key** of an experiment that names the browser by
  its templates (lab.md, 4.2): Chromium and the fonts directory in the
  lab's `browser`, the program among its `programs`. An experiment that
  names none keeps its key.
- **The lab's copies:** `~/src/rust/tmp/lab/browser/`, holding the
  program, built from skein (`cargo build --release -p
  skein-lab-browser`) and copied there, and `fonts/`, made by
  `scripts/lab/look/fonts` from fonts-dejavu-core's directory: the
  regular, bold and oblique faces of DejaVu Sans, Sans Mono and Serif
  it holds, the three regular ones required, and the runner's
  `look/aliases.conf` (the generic families and the browser's defaults,
  such as Arial, Helvetica, Times New Roman, Georgia and Courier New,
  to the three), each file `0400`, the directory `0500`. It prints its
  `fonts` member for the settings. Chromium is the one the lab names,
  by path; the machine's packaged browser is never used.
- **What the key leaves out:** Chromium's files beside its program
  (its `.pak` resources and ICU data) are stood for by its version, and
  the system libraries it loads (fontconfig, FreeType, the graphics
  stack) are the machine's, so they are not in the key. Shots under one
  key may still differ between machines; a capture's record says which
  machine made it only through the run's `about`.
- **By hand, for now:** until the look experiment (qa-web.md, section
  5) runs it as its collection, the capture runs on demand, with the
  same paths. It runs in no gate, and its shots are evidence, never
  goldens.

## 8. Open questions

- **The hand:** the program's calls on a socket in its directory, the
  person's acts through the face, and the sign-in (qa-web.md, section
  6).
- **A dark scheme** for a product whose look follows the system's.
- **Pages that animate** past reduced motion: readiness by a part
  shown holds for a static page; a page that moves after it would need
  an expectation of its own.
- **Chromium's own requests:** its background services (`www.google.com`,
  `content-autofill.googleapis.com`, `clients2.google.com`) reach the
  proxy and are refused, so `trouble` lists them as `network` beside the
  page's own (temper's chats mockup, at M2's hand capture). The look's
  checks (M3) must tell the page's from Chromium's, or the program turn
  those services off.
