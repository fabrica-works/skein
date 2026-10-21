# The browser parts

Provisional, 2026-10-15, revised 2026-10-19. The design of
`skein-ui-web`, skein's browser parts; of `skein-ui-bundle`, their
bundler; of the face and the harness in `skein-fake-person-browser`;
and of the UI example's browser tier. The tree the parts map is
ui.md's. What waits is in section 15, what is open in section 16, and
what stays fixed in section 17.

## 1. In one page

- **A toolkit, not a framework:** `skein-ui-web` is skein's browser
  parts, ordinary Rust, each usable alone, taking and giving plain data;
  it runs no loop and holds no application (5.1).
- **The application owns its start and its loop:** its browser shell,
  copied from the reference shell, drives its own state machine over the
  parts, and may change more by steps, up to a part replaced (section 3).
- **The parts hold the invariants that protect requests,** whatever the
  loop's order (5.3); the rest of the order is the reference's (5.2).
- **The tree as elements** by one default table, overridden by an
  owner's elements or rendered by its components (section 6); diffed in
  place, so focus, caret, selection, composition and scroll stay
  (section 7); scenes drawn as SVG, the fallback kept (section 8).
- **Its paths are the application's,** by its own codec; skein reserves
  none (section 9). **Effects come back through the inbox,** one report
  each, a person's gesture kept (section 11).
- **Built by skein's bundler,** with no wasm-pack and no npm: a page
  beside one directory named by its digest, under the application's
  prefix (section 4); **served by any server that keeps one contract**
  of types, caching and a strict policy (4.4).
- **Tested** on the host part by part, and in Chromium through the
  harness, its face and skein's stories for a browser shell (section 14).
- **What stays fixed,** with its reason and what to do instead: section
  17. **Not the parts':** signing in, cookies, an application's wire,
  its server's routes and the rest of its policy (section 2).

## 2. Where it sits

```
a person, a screen reader
  ▲ elements, focus, keys, the address bar, back and forward
the browser: fetch, session storage, history, clock, date, entropy
  ▲ the glue: wasm-bindgen, js-sys, web-sys
skein-ui-web             skein's browser parts, each usable alone (5.1)
  ▲ plain data: reports, patches, requests, paths, the look
<application>-shell-web  its start and its loop: the turn (5.2)
  ▲ its state machine's functions, called directly
its state machine        its state, its view over skein-ui, its codec
  ▲ its wire's bytes: exchanges, one stream
═══════════════════════════ one origin ═══════════════════════════
its server               the bundle (4.4); its exchanges and its stream
```

- **A person** reads the page by sight or with a screen reader, and acts
  on it with a pointer and keys. **The browser** is any current one,
  which the parts target; the browser tier drives Chromium (14.1).
- **`skein-ui-web`,** ordinary Rust: the web's mapping of the tree
  (ui.md, 16.1; section 6) and the platform's parts, knowing no
  application. It depends on `skein-ui` and lib, and, on wasm32, on the
  glue. **The application's browser shell,** compiled to the module, is
  its start and its loop (3.1, section 5), with no trait between it and
  the parts or its state machine.
- **Its state machine and its server** are the application's, whatever
  they are: its state, its view built with skein-ui (ui.md, section 5),
  its codec for paths (section 9) and its wire; a server on skein's HTTP
  (http.md, section 5) or any other, serving the bundle as 4.4 says. The
  UI example's state machine is a step machine in its own crate,
  `skein-ui-example-view` (examples.md, section 9).
- **Where the parts stop:** at bytes, to the paths the application
  names (section 10). What they carry, signing in, cookies, routes and
  any policy past 4.4's are the application's: its state machine's, its
  server's, or a part of its own (3.3, step 8).

## 3. What an application writes

Its extension points are the tree's (ui.md): its pages, tokens, region
names, pictures, drawings, components, effects, shortcuts and
phrasebooks. In a browser it writes its browser shell (3.1), gives the
parts its look (3.2), and changes what else it needs by 3.3's steps.

### 3.1 Its browser shell

- **`<application>-shell-web`,** its own crate, ordinary Rust, as shells
  are. Its start, through `wasm-bindgen`'s start attribute, which
  `skein-ui-web` re-exports so its version is skein's, makes the parts
  with the look (3.2) and starts its state machine (section 5); its loop
  is the turn (5.2). It orders the platform's work and decides nothing:
  what to show, send and keep is its state machine's; carrying a request
  safely is a part's (5.3).
- **The reference shell,** `skein-ui-example-shell-web`, the UI
  example's: about 200 lines (a guide), commented to be copied.
- **What the reference asks of the example's state machine:** *start,*
  from the seed, the path and query the page opened at, the small
  store's records, the person's offset and languages;
  *step,* with one report, giving its duties in order (the small store's
  writes and removals, exchanges sent and abandoned, the stream opened
  and closed, a navigation away, an address shown, new or in place, its
  effects); *build,* patches when its count of what is shown moved
  (ui.md, section 7); and the last tree, the path a link node leads to
  without acting (ui.md, 6.2), its next deadline, whether closing the
  tab would lose requests, and its language. A state machine of another
  shape changes these calls, and keeps 5.3.

### 3.2 Its look

The look (ui.md, section 8), as plain data the parts take when the shell
makes them, with no trait:

- **each owner's prefix,** and **the names of its style tokens and
  region names,** each starting with the prefix and a hyphen: classes
  and `data-region` values (section 6);
- **its pictures,** each picture's number to a file in the bundle's
  `pictures/<owner prefix>/` (4.2), or none;
- **its drawing code,** `draw`, a function routing a drawing by its
  kind's owner (ui.md, 11.1; section 8);
- **its components and its elements,** if any (6.1).

Beside it, the shell gives the parts **its name,** for the title
(section 7), and **the parts' own words** in the person's language
(section 5), from its phrasebooks (ui.md, section 9). Its stylesheets,
pictures, page and files go to the bundler (4.2).

### 3.3 What it may change

Steps from the standard, each needing one before it only where it says.

1. **Copy the reference shell** and change its names: the standard, the
   same for every application.
2. **Restyle:** its stylesheets after `skein-ui.css`, styling its tokens
   and placing its regions (section 12); its pictures (4.2).
3. **Its paths:** its own codec, at the root or under a prefix of its
   own (section 9).
4. **Its page:** its own `index.html` (its head: title, icons, fonts,
   metadata, preloads; its mount element; its stylesheets in its
   order), copied with one marker replaced (4.3), loading the starter
   by a script file, with nothing inline: the strict policy refuses at
   run what breaks that (4.4).
5. **Its files:** any of a type in the bundle's closed list, in the
   digest directory's `files/` (4.2); and root files, paths outside the
   prefix (`/favicon.ico`, `/robots.txt`), which its server answers from
   them (4.4).
6. **Its elements and components** (6.1): another element for a role
   carrying one of its style tokens; a component for such a node or for
   a drawing of its kinds: a code editor for a text area, a map for a
   drawing.
7. **Its JavaScript,** from the origin only: `wasm-bindgen`'s snippets,
   its own modules the glue imports (4.2), or files its page loads
   (step 4); never from another origin, never inline (section 17).
8. **A part replaced or left out:** any. One that holds an invariant
   (the transport, the inbox, the page) takes it with it to the
   application (5.3), which skein's stories for a browser shell check
   (14.2). There is no shell to write whole: the browser shell is its own.

## 4. The toolchain and the bundle

### 4.1 The toolchain

- **`wasm32-unknown-unknown`, stable Rust,** and the glue crates,
  `wasm-bindgen`, `js-sys` and `web-sys` (only the features the parts
  use), pinned to exact versions in skein's workspace: the only `unsafe`
  the module runs, with the code their macros generate
  (programming-model.md, 2.1). `skein-ui-web` forbids `unsafe`; an
  application's browser shell writes none of its own, and holds only
  what the glue's macros generate for its start and for bindings to its
  own JavaScript (3.3, step 7).
- **The glue** is generated by `wasm-bindgen`'s own library,
  `wasm-bindgen-cli-support`, which the bundler calls, pinned to the
  macros' exact version; `--target web`, an ES module. **The bundler is
  a dependency of the module's own workspace,** so one `Cargo.lock`
  locks the library with the macros. No other build tool: no wasm-pack,
  no npm, no binaryen.
- **The release module** is built with a profile that optimizes for
  size. The example's is `web`: it inherits `release`, with
  `opt-level = "s"`, `lto = "fat"`, `codegen-units = 1`,
  `panic = "abort"`, `overflow-checks = true` and `strip = true`. An
  application's workspace defines its own the same way. The browser
  tier's module is built with the dev profile, as code under test; the
  bundler, a tool, optimized (testing-strategy.md, section 8).
- **The parts' two halves:** each part's pure half builds for the host
  too, and is tested there (section 14); its platform half, like an
  application's browser shell, builds for wasm32 only.

### 4.2 The bundler and the bundle

- **`skein-ui-bundle`,** a tool, ordinary Rust, a library with a small
  binary, never linked into a service or a module. It makes the bundle
  from the module (a `.wasm` file cargo built), the application's name,
  the bundle's prefix (4.3), its stylesheets in order, each owner's
  pictures' directory, and its page and its files where it has them
  (3.3, steps 4 and 5).
- **The bundle is a directory, written whole:**
  - `index.html`, the page, the bundler's or the application's (4.3);
  - beside it, one directory **named by the digest of everything in
    it:** SHA-256 over its files' paths and bytes, in the order of their
    paths, its first 16 hex digits. It holds:
    - `boot.js`, the starter (4.3);
    - `<name>.js`, `<name>_bg.wasm` and `snippets/`: the glue, the
      module and the modules the glue imports (3.3, step 7);
    - `skein-ui.css` (section 12), then each given stylesheet, by its
      file name, in the order given;
    - `pictures/<owner prefix>/`, each owner's pictures, so two owners'
      never clash;
    - `files/`, the application's files.
- **The same inputs give the same bundle,** byte for byte; a change to
  any file in the directory gives another digest, and so another
  directory.
- **Only the closed list of types** (4.4): a file whose extension is not
  in it refuses the bundle, naming it; so do a stylesheet named
  `skein-ui.css` and two stylesheets of one name.
- **skein's texts that are not Rust:** the page and the starter, which
  the bundler writes, and `skein-ui.css`. Nothing inline.
- **Its size is a guide:** the example's module, built with `web`, has
  its size kept beside the example (examples.md, 9.4); past it, what
  grew is reported.

### 4.3 The page and the starter

- **The bundler's page,** `index.html`: `<meta charset="utf-8">`, a
  viewport, a title of the application's name, `skein-ui.css` then the
  application's stylesheets by `<link rel="stylesheet">`, the starter as
  `<script type="module">` by `src`, an empty mount element,
  `<div id="app">`, which the reference shell finds by its id, and a
  `<noscript>` saying the application needs WebAssembly and scripts, in
  words the application gives the bundler.
- **The page names the digest directory under the bundle's prefix,** by
  absolute path (`<prefix><digest>/boot.js`), since it is answered at
  every path the application does not reserve (4.4). The prefix is a
  parameter the application gives the bundler: a path of one segment or
  more, with one leading and one trailing `/` (`/static/`).
- **An application's own page** is copied as bytes, the bundler reading
  no HTML, with each occurrence of one marker, `{bundle}`, replaced by
  the digest directory's path under the prefix (`<prefix><digest>/`).
  Its references to the starter and to the stylesheets go through the
  marker; a page without it is refused, naming the page.
- **The starter,** `boot.js`: two statements,
  `import init from "./<name>.js"; init();`, passing nothing: the glue
  finds the module beside it by its own address. The starter, the glue,
  the module and the snippets reference each other relatively, so the
  directory is whole under any prefix.
- **The parts find the digest directory** as the directory of the
  starter's `src` on the page, and give the pictures' addresses under it
  (section 6).

### 4.4 Serving a bundle

The contract any server keeps that serves a bundle, whatever else it
serves. The example's server keeps it over skein's HTTP (examples.md,
section 9), in code an application's server may copy.

- **Under the prefix, the bundle alone:** each file of the digest
  directory at `<prefix><digest>/<its path>`, with `Cache-Control:
  public, max-age=31536000, immutable`; any other path under the
  prefix, another digest's among them, not found.
- **The page,** at every GET of a path the application does not reserve
  (its exchanges', its stream's, its root files', its own routes'), with
  an `ETag`, a digest of its bytes, and `Cache-Control: no-cache`, a
  matching `If-None-Match` answered `304`; **a root file** the
  application names, one of the bundle's `files/`, cached as the page
  is. **So a repeat load revalidates the page alone, and a page always
  loads one coherent bundle:** it names one digest directory, whose
  files never change under their paths.
- **Each file's type** comes from the bundle's closed list, by its
  extension: `.html` as `text/html; charset=utf-8`, `.js` as
  `text/javascript`, `.wasm` as `application/wasm`, `.css` as
  `text/css`, `.svg` as `image/svg+xml`, `.png` as `image/png`, `.webp`
  as `image/webp`, `.ico` as `image/x-icon`, `.woff2` as `font/woff2`,
  `.txt` as `text/plain; charset=utf-8`, `.webmanifest` as
  `application/manifest+json`; with `X-Content-Type-Options: nosniff`
  on every answer.
- **The strict policy the parts run under:** they and the bundle need
  no more than `Content-Security-Policy: script-src 'self'
  'wasm-unsafe-eval'; style-src 'self'`, with nothing inline: no inline
  script or style, no `style` attribute, no `eval`, no nonce or hash.
  The example's server sends, on every answer, the policy an
  application's server starts from: `default-src 'none'; script-src
  'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self'; font-src
  'self'; connect-src 'self'; base-uri 'none'; form-action 'none';
  frame-ancestors 'none'`. What a server adds for its own data (an
  image's `https` address needs its origin in `img-src`: section 6) is
  its own; `script-src` and `style-src` take nothing more (section 17).

## 5. The loop, the application's

Run by the application's browser shell over skein's parts (5.1), in the
reference's order (5.2) or its own, within 5.3.

- **Starting,** the reference's: the starter loads the glue, the glue
  the module, whose start makes the parts and reads through them, once,
  the clock, the date and what its state machine's start takes (3.1);
  then it starts the state machine, builds, and applies the first
  patches, inserting the whole tree under the mount element.
- **A panic is fail-stop** (programming-model.md, 2.1): the restart
  part reloads the page, which starts the application again from its
  small store, at most three times in a minute (a default, the
  application's parameter), counted by the date under a session storage
  key outside the small store's prefix (the reference's:
  `skein-ui-restarts`). Past it, the page says the application failed,
  in the words the shell gives (3.2), starts nothing and keeps the small
  store; a reload past the minute starts it again. Before either, the
  restart part clears the closing warning (5.1). A page that panicked
  can send nothing more, so the warning would ask the person to stay for
  nothing; the small store keeps what the application kept, for its next
  start.

### 5.1 The parts

Each is ordinary Rust, usable alone, taking and giving plain data; the
transport is built over fetch, the small store and the hidden tab, the
page over scenes as SVG, each usable without it.

| Part | Takes | Gives |
|---|---|---|
| the inbox, `Inbox<R>` | the application's turn function, at start; reports, each wrapped into `R` | the reports, in order; it calls the turn when one waits and no turn runs, within the callback that raised it |
| the page | a mount element, the look, the mapping (skein's table, the application's elements and components), the primary modifier; patches and the tree | elements kept in place; acts on node ids; a drawing's box resized; the link nodes it holds |
| scenes as SVG | a scene, the look's names | SVG elements: the page's default for a drawing |
| fetch | exchanges (a path on the origin, the method, headers and body the application gives, a bound on the answer's body), abandons; the stream opened (a path, its headers, a bound on a piece) and closed; the exchanges it may hold in flight | an exchange answered (its status and its body) or failed; the stream opened, refused (a status other than 200), a piece, its end; one read outstanding, the next asked by the loop |
| the small store | records by key, the application's prefix, its own bound | the records read at start; a write done or failed, synchronously |
| the transport | a step's writes, exchanges, the stream opened or closed, and a navigation away, whole | the store's writes first, their outcome reported; then exchanges and the stream through fetch (an opening while hidden, through the hidden tab), or every exchange of the step failed when a write failed; the navigation only after the step's writes, none when one failed |
| history | a path to show, as a new entry or in place | the path and query the page opened at; a place gone to by back or forward, as a path; the scroll kept per entry, given back after patches |
| the timer | a deadline | one timer, reported due |
| the platform's facts | nothing | the monotonic clock, wall time, a seed from the cryptographic generator, the person's offset and languages, the primary modifier |
| title and closing | a title, the language; whether closing loses requests | the document's title and `lang`; `beforeunload` registered only while it would, as the window's `onbeforeunload` |
| restart | the words for "the application failed"; its limit, three reloads in a minute by default; its key | the panic hook: the window's `onbeforeunload` cleared; a reload within the limit; past it, the page says so and starts nothing |
| the hidden tab | its delay, a minute by default; the tab hidden or shown; an opening of the stream while hidden | a deadline, past which fetch closes the stream; the opening held until shown, then made through fetch |

- **A report** is one platform happening, as data, in the type of the
  part that raised it (the table's "Gives"), or an effect's (section 11).
- **The primary modifier** (ui.md, 6.1) is Meta, a browser's name for
  Command, when `navigator.platform` names one of Apple's platforms (it
  starts with `Mac`, `iPhone`, `iPad` or `iPod`), and Control otherwise,
  an unreadable platform among it. The shell reads it once, at start,
  and gives it to the page part when it makes it; the page part writes
  shortcuts (section 6) and matches keys (section 7) by it, its pure half
  reading no platform.
- **The closing warning** is the window's `onbeforeunload` handler. The
  title and closing part sets it while the state machine says closing
  would lose requests, and clears it otherwise. Because it is the
  window's one handler, the restart part clears it without holding the
  title and closing part. The browser asks the person whether to leave
  only on a page they have acted on since it loaded.
- **The inbox is generic over the application's report type,** `R`:
  each part that reports is given, when it is made, the inbox and a
  function wrapping its report into `R`, the application's variant for
  that part. The turn matches `R` whole; a part the application adds, or
  its effects' code, reports the same way.

### 5.2 The turn

The reference's; an application's loop may order it otherwise within
5.3.

```
the turn, which the inbox calls when a report waits and no turn runs:
  now, wall ← the platform's facts                  once per turn
  while the inbox holds a report:                   raised meanwhile too
    a drawing resized: to the page part, no step
    the tab hidden or shown: to the hidden tab part, no step
    the timer due: to the hidden tab part, then a step as any report
    step ← the state machine steps with the report, now and wall
    the transport carries the step's writes, requests, navigation (5.3)
    fetch reads the stream on, after a piece of it
    history shows the step's address, printed by its codec
    the step's effects, to their parts or the shell's own code (11)
  patches ← the state machine builds
  the page applies them, a focus request too        an unknown node: a panic
  a new entry: its top, its heading focused, unless a focus request moved
  back or forward: the scroll the entry kept
  the page sets each link's href to the path the codec prints
  title and closing: heading · name, the language, the closing warning
  the timer: the earliest of the state machine's and the hidden tab's
```

### 5.3 Where the invariants live

The parts hold the invariants that protect requests, so no order of the
loop breaks them; the reference documents the rest.

- **The small store written before a step's exchanges leave, and a
  navigation away only after:** the transport's. A state machine that
  keeps a request in the small store before sending it, so that a
  reload sends it again, needs the keep done before the send leaves,
  and a failed keep to send nothing (section 10). An application driving
  fetch and the small store without the transport takes the rule on
  itself, which skein's stories for a browser shell check (14.2).
- **No re-entry:** the inbox never calls the turn while one runs; a
  report raised during a turn (a write's outcome, a focus moved, an
  element removed) is taken by the same turn, in order. The reference
  keeps its state in one cell, whose second borrow panics, fail-stop.
- **Patches applied to the tree they are diffed from:** the page part
  refuses, as a panic, fail-stop, a patch naming a node it does not
  hold, so a build whose patches are dropped fails at the next apply.
- **The rest of the order** (links' `href`s, title, timer and closing
  warning after the build) is the reference's (5.2): out of order, a
  turn shows something stale until the next, a closing warning among
  it; no request leaves unkept or twice.
- **Why not types.** A typestate that made the wrong order unwritable
  would make the parts' invariants the shape of every application's
  loop, a framework by its types, and would not stop an application
  calling the platform itself; a part that holds its invariant asks
  nothing of the loop but to be used. Documented order alone would leave
  an application's code ordering what the parts promise.

## 6. The tree as elements

Each node is one element, by its role, through the page part's default
mapping, this table (ui.md, 3.1, 3.2 and 16.1); the browser's face finds
the person's roles through the same table (14.2). An application
overrides it for its own nodes only (6.1). A role, state or property the
light path adds (ui.md, 4.3) adds its row here, with its golden row in
the host tests (section 14).

| Role | Element (ARIA role where the element has none) |
|---|---|
| page | the mount element the page part is given (4.3) |
| banner, navigation, main, complementary, content info | `<header>`, `<nav>`, `<main>`, `<aside>`, `<footer>` |
| form, search | `<form>` (named), `<search>` |
| region | `<section>`, named |
| heading | `<h1>` to `<h6>`; deeper, `role="heading"` with `aria-level` |
| paragraph, text | `<p>`, `<span>` |
| list, item | `<ul>`, `<li>` |
| table, row, cell, column header, row header | `<table>`, `<tr>`, `<td>`, `<th scope="col">`, `<th scope="row">` |
| term, description | `role="term"`, `role="definition"` |
| button | `<button type="button">` |
| link | `<a href="<path>">`; a checked external Markdown link, `<a href="<url>" rel="noopener noreferrer" target="_blank">` |
| text field | a visible `<label for>` before a named `<input>`, its `type` by its entry kind: `text`, `number`, `email`, `tel` for a telephone, `url`, `date`, `time`, `password` for a secret |
| text area | `<textarea>` |
| choice, choice group | `<input type="radio">` with its label, `role="radiogroup"` |
| check | `<input type="checkbox">` with its label |
| select, option | `<select>`, `<option>` showing its name as its text |
| menu, menu item | `role="menu"` behind its button, `role="menuitem"` |
| disclosure | `<details>` with `<summary>` |
| dialog | `<dialog>`, shown modal |
| status, alert, log | `role="status"`, `role="alert"`, `role="log"` |
| tree, tree item | `role="tree"`, `role="treeitem"`; nested tree items in `<div role="group">` |
| tabs, tab | `role="tablist"`, `<a role="tab">` |
| progress | `role="progressbar"` with its text as `aria-valuetext` |
| time | `<time>` |
| code, quote, separator | `<pre><code>`, `<blockquote>`, `<hr>` |
| image | one of its owner's pictures: `<img src="<prefix><digest>/pictures/<owner prefix>/<file>" alt="<name>">`; an address: `<img src="<the address>" alt="<name>">`; a picture the look lacks: its name as text |
| figure | `<figure>` |
| slot | `<div>`, its children the owner's nodes |
| drawing | `<figure>` named by its name: the scene's `<svg aria-hidden="true">`, then the fallback's elements, hidden from sight but not from assistive technology while the scene is drawn, shown when it is not (section 8) |

- **An ARIA role alone** in the table is a `<div>` with that role.
- **Nested tree items:** each run of consecutive tree-item children of a
  tree item sits in one unnamed `<div role="group">` with no class.
  Other children stay directly under the tree item, in document order.
  A leaf has no group, and removing the last item removes its group.
  The browser derives levels and set sizes from this nesting where the
  owner gives no explicit count or position.
- **Attributes:**
  - *a name* is `aria-label` where the element's own text does not give
    it; a button, a tab, a menu item and an option show their name as
    their text; a named text field has a visible `<label>` just before
    its input, linked by `for` to `id="skein-ui-label-<node id>"`, as a
    choice or check is linked to its label. The label supplies the name
    once, with no `aria-label` on the input; an unnamed field has no label;
  - *a description* is `aria-describedby`, naming an element of the page
    part's that holds its words as text, shown beside the node's
    element: after it; inside it, last, for an item, a cell and a
    header, whose parents take no other child; after the `<select>` for
    an option, which holds text alone;
  - *states:* disabled is `disabled` where the element takes it, else
    `aria-disabled`; expanded `aria-expanded`; busy `aria-busy`; current
    `aria-current`, `page` on a link; invalid `aria-invalid`; pressed
    `aria-pressed`, a check's `checked`; selected `aria-selected`, a
    choice's `checked`, an option's `selected`; required `required`
    where the element takes it, else `aria-required`; submits on enter
    `enterkeyhint="send"`. There is no provisional state (ui.md, 3.2),
    and no attribute stands for one;
  - *a focus request* is no attribute: the page part moves the focus
    (section 7);
  - *a collection's count and an item's position:* a list's or a tree's
    count is `aria-setsize` on each of its items or tree items, an
    item's position `aria-posinset`; a table's count is `aria-rowcount`,
    a row's position `aria-rowindex`;
  - *a cell's spans* are `colspan` and `rowspan`, left out at 1; *a
    column header's sort* is `aria-sort`, `ascending` or `descending`;
  - *a region name* is `data-region="<its name>"`; *style tokens* are
    classes, by their names (section 12); *a shortcut* is
    `aria-keyshortcuts`, in ARIA's form (`Alt+K`, `K`, `Escape`), a
    primary one by the primary modifier the page part was given (5.1):
    `Control+K`, or `Meta+K`, never both.
- **An image's address** is a path on the origin or an `https` URL,
  checked by skein-ui as Markdown's external links are (ui.md, 3.5);
  the `src` takes it as is. An `https` one needs its origin in the
  server's `img-src` (4.4), else the image shows its name. The name is
  its text alternative, always.
- **Values** are in the element's own formats (ui.md, 3.4): a number in
  decimal digits, a date `YYYY-MM-DD`, a time `HH:MM`. A value the
  element does not accept shows empty, and an `Input` carries what the
  element gives back: an `<input type="number">` holding no number gives
  the empty text, which the owner's view reads as no number.
- **Text runs:** plain as text, emphasis `<em>`, strong `<strong>`, code
  `<code>`, a link run `<a>`.
- **Text is text:** every name, description, run, value and mark's text
  is written as text, never read as markup (ui.md, section 10); bytes
  that are not UTF-8 show as U+FFFD.
- **What the page part sets, and so reserves:** `data-node`, a node's
  id, on its element; `data-region`; `data-drawn` on a drawing's figure
  while its scene is drawn; `data-act` on a mark that carries an act
  (section 8); and ids starting `skein-ui-` on its own elements, which
  a description's `aria-describedby` and a menu button's `aria-controls`
  name. It reserves no class name. The parts' own elements (the mount
  element, a scene's SVG, a menu's button, a description's element, the
  restart part's words) carry no `data-node`.
- **One listener per kind of event,** on the mount element, finds the
  node of the element acted on, by its nearest `data-node`. A click
  finds instead the nearest element whose node a person presses, a
  link, a button, a tab or a menu item, as a browser's own `<a>` and
  `<button>` take a click on their content (ui.md, section 3); a click
  inside none of them, its nearest `data-node`.

### 6.1 An application's elements and components

- **Another element,** for a role carrying one of its owner's style
  tokens (an `<article>` for a region carrying `notes-card`), keeps the
  node's role and name: the page part adds ARIA's role where the
  element's own differs. States, `data-node`, listeners and patches
  stay the page part's; nodes with no override keep the table.
- **A component** (ui.md, section 12), chosen by the node's role and one
  of its owner's style tokens, or by a drawing's kind, from the look's
  component table, is the application's ordinary Rust, given:
  - a host element the page part keeps in the node's place, with its
    `data-node`;
  - the node, on insert and on each patch to it;
  - the inbox and a function wrapping an `Act` on its node into the
    application's `R` (5.1).

  It builds what it likes inside the host, reports only the acts its
  node's role takes, on its node (an `Input` with a field's whole value,
  a `Drawing` with its owner's bytes), and is removed with its node. The
  view refuses any other act, and any on a disabled node (ui.md,
  section 6), so a component on a role that does not fit is inert: **it
  renders a node and never stands in for a role.** The node and its
  fallback stay in the tree, which is all the text shell, the fake
  person and a screen reader see (ui.md, section 12); a drawing's
  fallback stays beside the host, hidden from sight only (section 8).
- **Inside the host element is the component's:** the page part's
  listeners leave its events to it; while the focus is inside, a
  shortcut with no modifier is not taken, as in a field.
- **What a component builds is the application's to keep accessible
  and safe:** the node's role and name on the platform (a code editor is
  a text area to a screen reader), checked by its tier; never a node's
  text read as markup (section 17); its styles from the origin's
  stylesheets, since the strict policy refuses a `<style>` element or a
  `style` attribute it writes (4.4).

## 7. Patches and acts

- **Patches applied in place** by the page part (ui.md, section 7):
  - *an insert* creates a subtree's elements under its parent, before
    its sibling or at the end; each link gets its `href` in the turn
    (section 9), each drawing its scene (section 8) or its component
    (6.1);
  - *a remove* removes the element and its subtree;
  - *a move* moves them, and gives back the focus and the selection of a
    field it moved;
  - *a text* replaces a node's runs;
  - *an attribute* sets or removes what changed: a drawing whose data
    changed is drawn again, and a role whose element differs makes the
    new element, the children moved into it;
  - *a value* writes a field's value, which comes only from the owner's
    write (ui.md, 3.4).
- **Never replaced while it can be updated:** focus, caret, selection,
  composition and scroll stay. A removal that takes the focus moves it
  to the nearest remaining ancestor that can take it, else the main.
- **A focus request:** after the apply, a node inserted with one, or
  whose count moved, takes the focus. An element that cannot take it by
  itself (a heading, a region) gets `tabindex="-1"` first, so it takes
  the focus without joining the tab order.
- **Acts,** each reported with its node's id (ui.md, section 6), none on
  a disabled node; the view refuses one that races a patch disabling
  its node, and an option of a disabled select is disabled:
  - a press on a button, a tab or a menu item: `Press`, which only
    they, a link and an option take (ui.md, section 6);
  - a plain press on a link: `Press`, and the page does not navigate. A
    press with a modifier, or the middle button, on a link is the
    platform's: it opens the link's path in a new tab, a new start of
    the application; so is any press on an external link;
  - an input's whole value, on each `input` event outside a composition,
    and once at its end: `Input`;
  - Enter, outside a composition, in a field that submits on enter:
    `Submit`, its whole value sent first as `Input`; Shift+Enter in such
    a text area inserts a line break where its owner allows it, and does
    not submit;
  - a check's or a choice's change: `Toggle`; a select's change:
    `Press` on the chosen option's node. The page part then puts the
    element's choice back to the tree's, which the next patch moves if
    the owner followed;
  - a press in a drawing's scene: `Drawing` (section 8).
- **Shortcuts** (ui.md, 6.1): the platform's keys turned into a press of
  the first bound, enabled node in preorder naming the key; a shortcut
  with no modifier never while a field has the focus. The page part's
  platform half gives each `keydown` as the event holds it, and its pure
  half decides:
  - *the modifier,* from the event's `ctrlKey`, `metaKey`, `altKey` and
    `shiftKey`, by the primary modifier (5.1): none held, none; the
    primary one alone, the primary; Alt alone, Alt; any other set, no
    shortcut;
  - *the key,* the event's `key`, lowercased, when it is a letter `a` to
    `z` or a digit; else the letter or digit its `code` names (`KeyK`,
    `Digit5`), as when Alt on macOS composes another character, or a
    layout gives other letters;
  - *a key that presses a node* has its default prevented, so that the
    browser does not act on it as well.
- **Escape:** the platform's Escape key in an open dialog, which the
  browser gives the `<dialog>` as its cancel event, presses the first
  bound, enabled node in the dialog naming Escape; with none, it does
  nothing. The page part prevents the dialog's own closing: a dialog
  closes when the tree removes it, and one the browser closes on its own
  while it is in the tree is shown again. An open menu takes Escape
  first, to close itself; a component that takes Escape for itself
  keeps it.
- **Menus and disclosures are the page part's to fold,** with no step. A
  disclosure is `<details>` and `<summary>`. A menu is a button of the
  page part's, named by the menu's name, that opens its items with the
  platform's keys (arrows, Escape): ARIA's menu-button pattern. A press
  on an item closes the menu. **A dialog** is shown modal while it is in
  the tree. **A log** keeps its end in view while the person has not
  scrolled away from it.
- **Going to another page** (a new entry a step shows): after the turn's
  patches, the reference starts the page at its top, the focus on its
  main's first level-1 heading, else the main, unless a focus request
  moved in the same apply; back and forward restore their scroll,
  history's (section 9).
- **The title,** as the reference sets it: the main's first level-1
  heading's text, then ` · ` and the application's name; the name alone
  when there is none. `<html lang>` is the phrasebook's language.

## 8. Scenes as SVG

A scene is drawing code's output: a bounded list of marks in preorder,
in whole pixels from its box's top left (ui.md, 11.2). Scenes as SVG is
a part, the page part's default for a drawing; an owner's kind may be a
component instead (6.1).

- **The page part calls the drawing code** (3.2; ui.md, 11.1) with the
  drawing's kind, data and box's size in whole CSS pixels, as it
  measured it; false, for a kind or data it does not draw, leaves the
  fallback shown.
- **Drawn as inline SVG:** an `<svg aria-hidden="true">` of the box's
  size, first in the drawing's `<figure>` and at its top left, so a
  point in the scene is the same point in the figure's box; one element
  per mark:

  | Mark | SVG |
  |---|---|
  | group | `<g>`, translated by its offset, its marks inside |
  | line, polyline | `<line>`, `<polyline>` |
  | path | `<path>`: its move, line, quadratic, cubic and close commands |
  | rectangle, circle | `<rect>` with its corner radius, `<circle>` |
  | text | `<text>`, anchored at its start, middle or end |

- **A mark's tokens are classes,** by their names; its hint, never
  needed to decide, is a `<title>` in it, shown on hover. Colours,
  widths and fonts are the stylesheets' (section 12): a scene says none.
- **Refused, never cut:** a scene past skein-ui's limits (marks and
  bytes per scene, a node's tokens per mark: ui.md, section 15) draws as
  its fallback.
- **Drawn again** when the drawing's data changes (one patch), or when
  its box's size changes, which is the page part's alone: reported, it
  calls the drawing code again, with no step (ui.md, 11.4).
- **The fallback** is the drawing node's children, ordinary elements.
  While the scene is drawn, the page part sets `data-drawn` on the
  figure, and `skein-ui.css` hides the fallback from sight, not from
  assistive technology, taking no space. The text shell and the fake
  person read the fallback, unchanged (ui-text.md, section 5; ui.md,
  17.1).
- **Acted on:** a press in the scene reports `Act::Drawing { data }` on
  the drawing's node, its data the act the press takes in ui.md, 11.3's
  order: the topmost mark's under the point, else its nearest group's,
  else the scene's with the point appended, else none. A mark that
  carries an act gets `data-act`, so that `skein-ui.css` has it take
  presses over its whole area, filled or not (section 12). The view
  drops data past its bound on a drawing's bytes (ui.md, section 6). It
  is by pointer only: the ordinary node beside the drawing, doing the
  same, is the keyboard's and a screen reader's.

## 9. Addresses and history

- **The application's paths:** its codec parses and prints every path
  the page shows, below its state: total, bounded step code, since a URL
  is a person's input. It says which place `/` is, and what a path with
  no place shows. The parts give it the path and query, a trailing
  slash and the query its to read or ignore, and no fragment.
- **Nothing reserved by skein:** the parts read and print no path of
  their own, and the page is answered at every path the application
  does not reserve (4.4): **the bundle's prefix,** given to the bundler
  (4.3), which its codec never prints as a place, and its exchanges',
  its stream's and its root files' paths. Its places under a prefix of
  their own (`/notes/…`) are its codec's change; it keeps its bookmarks
  by keeping its codec.
- **A link's `href`:** a link node's binding resolves, without acting,
  to the address it leads to (ui.md, 6.2), which the codec prints as a
  path. After each apply, the reference prints the path of every link
  node the page part holds, and the page part sets each `href` whose
  path changed, since a changed binding makes no patch (ui.md,
  section 7).
- **History:** an address a step shows is a new entry, or replaces the
  current one, as the step says. The history part keeps the scroll in
  each entry's state as the page leaves it. Back and forward are a place
  gone to, as a path: one step, then, after its patches, the scroll the
  entry kept. A path opened directly (a bookmark, an address typed, a
  link opened in a new tab) starts the application at it.

## 10. Fetch, the small store and the transport

- **Exchanges** by fetch, to a path on the origin, never to another
  origin: the method, headers and body the application gives (a POST
  with its body, a GET with none), with same-origin credentials;
  abandoned by aborting; at most the exchanges in flight fetch is given.
  An answer's body is read up to the bound the exchange gives; past it,
  the exchange failed. Which headers an exchange sends, what its body
  holds and what its server answers are the application's (a header
  naming its client, a cookie its server set).
- **The stream** by fetch, a GET of a path with the headers the
  application gives, with same-origin credentials, its body read a piece
  at a time: each piece, at most the bound the application gives (a
  longer one handed in parts), handed to the state machine as it comes,
  the next read asked by the loop after the step (5.2), so the platform
  paces it. One stream at a time.
- **The transport** takes a step's requests whole: the small store's
  writes first, then the exchanges and the stream, then the navigation
  away, if any, to a path on the origin or an `https` address the step
  names (a provider's sign-in, another site). When a write fails, every
  exchange of the step is reported failed, none is sent, and there is
  no navigation (5.3).
- **A hidden tab lets its stream go** a minute after it is hidden (a
  default, the application's parameter): fetch closes the stream and
  reports its end, as any drop; an opening while hidden waits until the
  tab is shown. A browser allows about six connections to one origin
  over HTTP/1.1, each tab's stream holding one.
- **The small store** is the tab's session storage: it survives a reload
  and goes with the tab. Each record and its key are base64 text
  (lib.md, 6.1), the key under the application's prefix, read whole at
  start. A write fails when the platform refuses it, its quota full, or
  past the small store's own bound, the application's parameter,
  unbounded by default, which a story sets low (14.2).

## 11. Effects in a browser

An effect (ui.md, section 13) is data an application's step emits, in
its own closed enum, asking its platform for what is outside the tree (a
share sheet, a clipboard write, a file picked).

- **Carried out by its shell:** its own code over the glue, or a part of
  skein's where one exists (the file picker, when it comes: section 15),
  giving back **exactly one report per effect,** carrying its token:
  done, refused, failed, or its result (a file's name, type and bytes,
  within the bound the effect states). One the shell cannot carry out is
  reported refused at once.
- **Through the inbox, like any report:** the code that carries out an
  effect is given the inbox and a function wrapping its report into `R`,
  as each part is (5.1). **No re-entry still holds:** a platform
  callback (a promise settled, a picker's change) raises a report, and
  the inbox calls the turn only when none runs.
- **A person's gesture:** an effect the platform allows only during one
  (a file picker, a share sheet, a clipboard write) is asked in the turn
  the person's act started, which the inbox runs within the act's event,
  so the gesture holds. Asked in a turn a timer or an answer started, it
  is refused by the platform, and reported refused.
- **A reload ends what is in flight:** an effect asked before it gives
  no report after it; the state machine starts again from its small
  store (section 5), which holds no effect in flight.

## 12. The stylesheet

- **`skein-ui.css`,** written by the bundler first in every bundle
  (4.2), and loaded first by the page. It:
  - hides a drawn drawing's fallback (`data-drawn`) from sight only,
    taking no space, and places a scene's SVG at its figure's top left
    (section 8);
  - gives every mark of a scene a default, strokes in the text's colour
    and no fill, and acting marks (`data-act`) `pointer-events: all`;
  - keeps focus outlines visible;
  - follows the platform's light or dark preference, and reduced motion.
- **It styles no token and places no region:** skein-ui reserves no
  token names, and layout is never the tree's; it sets no palette. **No
  style is a fact:** nothing a person must know is said by a class alone
  (ui.md, section 8).
- **An owner's stylesheet** loads after it: its tokens, as classes by
  their names; its regions, placed by `data-region` and the landmarks'
  elements; its palette, custom properties under its prefix. It may
  restyle what `skein-ui.css` gives. An application of several owners
  loads each one's, in the order it gives the bundler, or its own page's
  (3.3, step 4).

## 13. Limits

- **skein-ui's:** a tree's nodes, a diff's patches, a scene's marks and
  bytes, a drawing's bytes (ui.md, section 15); a drawing's SVG has at
  most a scene's marks.
- **The application's:** an answer's body, a piece of the stream,
  exchanges in flight, the small store's own bound (section 10).
- **The platform's:** session storage's quota, past which a write fails.
- **The parts' own:** one timer; one stream, with one read of it
  outstanding; the inbox's reports raised during a turn, only those the
  turn's own calls raise; three reloads in a minute, by default; a
  minute hidden before the stream goes, by default.
- **The bundle's size,** a guide (4.2).

## 14. Testing

Each within skein's suites' budgets (testing-strategy.md, section 8).

- **On the host,** in skein's default suites, each part's pure half: its
  step tests in its crate's `src/` (testing.md, section 6), and what
  needs more than one crate in `tests/ui-web`:
  - the mapping and its overrides, a golden row per role, state and
    property (section 6), a primary shortcut's under each primary
    modifier;
  - keys made shortcuts under each primary modifier (section 7): the
    primary one pressing; the other one, Shift besides, or Control with
    Alt pressing nothing; Alt's composed character read by its `code`;
  - patches applied to a model of the document, printed as HTML and
    compared with golden files of the example's pages: each kind of
    patch, a focus request, a check's and a select's choice put back,
    Escape in a dialog;
  - the small store's text (base64, keys under the prefix, a write past
    its bound failed); scenes as SVG, against golden SVG;
  - the transport's order over a fake platform: writes before exchanges,
    a failed write failing the step's exchanges and cancelling the
    navigation; the inbox's: no turn inside a turn;
  - the bundler: the directory named by its digest, the same inputs
    giving the same bundle, references under the prefix, an own page's
    marker, and each refusal of 4.2 and 4.3 naming its file.
- **The wasm32 builds in skein's gate:** `skein-ui`, `skein-ui-web`, the
  example's view and its browser shell, built for
  `wasm32-unknown-unknown` with clippy's lints as errors, since the
  parts' platform halves are compiled only there (testing.md, section 6;
  lib.md, section 2).

### 14.1 The browser tier

- **In the browser suite,** `--profile browser`, which skein's gate runs
  after it builds the example's bundle: the profile takes the binaries
  of `tests/ui-web` named `browser_*` beside the browser kit's own tests
  against Chromium, and its global timeout, two minutes, holds both
  (browser.md, section 9; testing.md, section 7). The example's tier
  aims at one minute of it on the slowest machine, a guide.
- **Each of the example's tests** starts one server, the example's
  binary, and one Chromium, and plays its stories with a person each: a
  browser context, its own cookies and storage (browser.md, section 5).
  The module is built with the dev profile, the bundler optimized (4.1).
- **A missing Chromium or bundle fails the suite,** naming the command
  that provides it; it is never skipped. The bundle's directory reaches
  the tests in `SKEIN_UI_EXAMPLE_BUNDLE`.
- **Under the strict policy:** the example's server sends 4.4's policy,
  and the harness fails a test on any violation (14.2).
- **The facts the face assumes,** checked against Chromium on fixed
  pages that a small server on skein's HTTP serves in the test's loop,
  as the kit checks its own (browser.md, section 9): the accessible role
  Chromium computes for each element and input type of section 6's
  table, which a find of that role asks for (14.2); a `<select>`'s
  option chosen by the kit's keys; and a date field and a time field
  given their values by the keys 14.2 names, in their parts' order
  under the kit's locale (browser.md, sections 3 and 6). A new Chromium
  version is a run of these, and a fact that changed is fixed in the
  face's table first.

### 14.2 The face and the harness

- **The face,** in `skein-fake-person-browser`: the person's `Find`
  (ui.md, 17.1) over skein's browser kit:
  - roles through section 6's mapping, which an override keeps (6.1):
    for each role, the accessible roles its elements compute in
    Chromium, which a find of that role asks the kit for (a text field:
    `textbox`; `spinbutton` for a number's, `Date` for a date's and
    `InputTime` for a time's; 14.1);
  - names by words in them or exactly (browser.md, section 5); text by
    words within one run, a sentence that a strong word or a link splits
    being several runs;
  - `Pending` while the kit answers.
- **Its acts:** press; type, an input's whole value (the field cleared,
  then the words), a date's or a time's by the kit's keys (below);
  Enter for a send; a key with Shift held, Shift+Enter a new line in a
  text area that sends on Enter (section 7); an option chosen with the
  kit's keys, as a keyboard chooses one; `Go`, a navigation to the path
  the application's codec prints for the address; a reload. A go and a
  reload accept the page's closing warning, as a person who chose to go
  answers it (browser.md, section 6).
- **It reads the current address,** the kit's `Address`, from the page's
  navigation history (browser.md, section 6), as the address bar shows
  it: what the application's codec printed, or what its page replaced,
  such as a sign-in's secret taken from the address. A story reads it
  once what the page shows says the page has put it there, never at a
  fixed time.
- **A date or a time is typed as a person types it,** into its native
  field's parts, since that field takes no inserted text (browser.md,
  section 6). The face knows the field by the role Chromium computes
  for it, `Date` or `InputTime`. It turns the value, in the element's
  own format (section 6), into the keys of the field's parts under the
  kit's locale, `en-US` (browser.md, section 3), and gives them to the
  kit's `keys`:
  - a date `YYYY-MM-DD` is typed as its month, its day, then its year:
    `2026-10-15` as `10152026`;
  - a time `HH:MM` is typed as its hour on a 12-hour clock, its
    minutes, then `A` before noon or `P` from noon: `14:30` as `0230P`,
    `00:00` as `1200A`, `12:00` as `1200P`;
  - the empty value is typed as no key;
  - any other value fails the step, naming the field and the value,
    since no person's keys put it in that field.

  The order is a fact the face assumes (14.1). A Chromium that orders
  the parts otherwise fails that fact, which names the field it typed.
- **The harness,** in the same crate, ordinary Rust, runs the
  application's server binary and Chromium as children of one loop,
  through skein's io (testing-strategy.md, 2.9); opens the address the
  test gives, or the line the binary prints on its standard output or
  error, as the test says, found by a prefix the test gives: an address,
  opened at its root, or a whole URL, opened as printed, each person
  after the first at a line no earlier person opened, since a printed
  URL may be good once, or, when the test says so, holds the line back
  from Chromium for a command's standard input (below), the person's
  page then blank until a step opens a link; runs each person's script
  through the face;
  fails a test on any trouble the kit reports that the story did not
  expect, a policy violation, an exception or a native dialog
  (browser.md, section 7); and writes the accessibility snapshot, a
  screenshot, and Chromium's and the binary's stderr's tails on a
  failure (browser.md, section 10).
  **It hosts the peers a test lends it,** in the same loop, so that a
  story reaches a fake service the test holds, such as skein's forge
  peer (fake-forge.md, sections 5 and 10.5):
  - a peer is any of skein's fake peers that listens:
    `skein_fake_peers::Listening`, a `Host` (shell.md, section 12) with
    the address it listens at and its `shutdown`, which the forge, OAuth
    and LLM peers meet. The harness names no forge and no application;
  - the test builds each peer over the state it keeps a handle to, the
    forge's shared handle for the forge peer, and lends it to the run
    under a name of its own;
  - the harness opens its kernel with room for the peers' operations
    beside its own, and drives each peer as a shell does: it reaps the
    peer's completions, iterates it, and submits what it asks, each pass,
    the peer's deadlines among the loop's. A token is the peer's own, as
    in a world's real loop (testing-strategy.md, 2.9), and never meets
    the harness's;
  - nothing else starts until each peer listens. Then each `{name}` in
    the server's arguments, in a command's (below) and in the origins'
    pages is that peer's address, `ip:port`, as `{address}` is the
    server's;
  - a peer's failed assertion fails the test as the binary's trouble
    does, naming the peer;
  - at the run's end the harness shuts each peer down and drives it
    until it holds nothing, within the run's time, and hands it back.
    The test then reads the peer's observations, and the state behind
    its handle, as a world's test reads them after its run.
  At a story's step it also:
  - presses a drawing's mark, which the person cannot: the kit's press
    at the mark's point on the drawing's figure (section 8; browser.md,
    sections 5 and 6);
  - **stops the server binary and starts it again** at the address it
    had, which its arguments name, so that a page sees its server go and
    come back;
  - **opens a page of another origin,** which the harness serves itself:
    fixed pages the test gives, on listeners of its own in its loop, at
    a host name the test gives (another port of the server's host, or
    another host name), each naming the server's address where the page
    asks, so that a story checks the server against other sites;
  - **sends a request of its own** to the server, through skein's HTTP
    client, with the method, path, `Host`, headers and body the test
    gives, and awaits its status;
  - **runs a command,** as a person types one into a terminal beside
    the browser:
    - the command is a child of the loop, as the server is, with the
      program, arguments and environment the test gives, each
      `{address}` the server's and each peer's `{name}` its address;
    - its standard input is closed, or, when the test says so, is the
      line the server printed and the test held back from Chromium
      (below), then closed. So a command that signs a person in by the
      server's link spends it, and the person's page never sees it;
    - the harness keeps the command's output whole, within the run's
      bounds, after it exits as while it runs. A command that prints a
      link and exits before any page opens it is a command like
      another;
    - when the test gives a prefix, the step awaits, within its time,
      the first line that starts with it on the output the test names,
      whether the command runs on or has exited. It opens that line in
      the person's page as a whole URL, as printed, and keeps it under
      a name the test gives (below). A command that exits without the
      line fails the step, naming it;
    - a later step awaits the command's exit, with the code the test
      gives, within its time, so that the story goes on while a command
      that waits for the browser's return runs;
    - each person runs one command at a time, and runs several in turn.
      A command that has not exited when its story ends fails the test,
      naming it, and the harness kills it. Its standard error's tail is
      written on a failure, as the binary's is;
  - **keeps a value for the test:** an attribute the test names, or
    else the `value` attribute, of the first element a CSS selector the
    test gives finds in the person's page, as its document holds it at
    the step (browser.md, sections 5 and 6), under a name the test
    gives. A selector, not a target, since a hidden field holds no role
    a person finds it by, and its `value` attribute is its value. It
    reads markup: what a person typed into a field a find reads, by its
    target. A selector that finds no element, or an element without
    that attribute, fails the step. The run hands back what each person kept, by name, with each
    link a command's step opened, so that the test reads a peer's state
    by what the story saw;
  - **awaits trouble** with given words, which the person's pages then
    expect from the script's start, so that it fails no story;
  - **opens again the URL** the person's page first opened, so that a
    story sees a printed URL spent;
  - **checks whether the person's last go met its page's closing
    warning** (browser.md, section 6), so that a story sees the warning
    come while closing would lose requests, and not otherwise (5.1). A
    story that looks for no warning has its person act on the page
    first, since the browser warns only on a page acted on since it
    loaded.
- **skein's stories for a browser shell,** functions of the harness an
  application's tier calls with its binary, its bundle and its codec,
  and what each story names of its pages, to check its wiring, since
  its loop is its own:

  | Story | What it checks | What the application names |
  |---|---|---|
  | typing while a patch arrives | the field keeps its focus, its caret and its words | a field; a second person's script whose act patches the first person's page; what that patch shows there, which the page does not show before it; and what a page shows once it holds its server's state |
  | back and forward | they move between pages, and an address opened directly shows its page | two addresses, and what each page shows; and what a page shows once it holds its server's state, which the story awaits at its start before its first move, since an application may sign the person in on its first page |
  | a panic | the page reloads within the restart limit, and past it says the application failed | an address at which its module panics at start, as its test build allows; and its words for "the application failed" (5.1), which its page then shows |
  | a write refused | a write the small store refuses fails the step's exchanges, and none leaves | an address at which its small store is bounded below a step's write, as its test build allows; a script making that step there; what the step would have made, which no page then shows; and what a page shows once it holds its server's state |

- **What the application names is what a story takes,** and nothing
  else. What a story must see on a person's page is named as what a
  person looks for (ui.md, 17.1), and the story awaits it: never for a
  fixed time, and never by skein's words where the page shows the
  application's, as the restart part does (5.1). A story looks for
  something absent only once its page shows that it holds its server's
  state, which the application names too: what a page has not read yet
  is absent as well, and the page's load, which the kit sees, comes
  before its first read is answered. Seeing the patch is
  also what shows that one came: with no patch, a field keeps its
  focus, its caret and its words anyway. The panic's module panics at
  start, so that each reload panics again and the page reaches the
  limit with no person acting; a script's act, made once, would not.
- **An application's tier** calls them, and adds its own over the same
  harness and face: its pages, drawings, components and effects, and
  what its server does past the parts (its sign-in).

### 14.3 The example

The UI example (examples.md, section 9) carries one of each thing the
parts carry, so that its tier tests each once.

- **Its tier** calls skein's stories, giving them its title field, a
  second person adding a note, and that note in the first person's
  list; `/` and a note's address, and what each shows; and, in its test
  build alone, an address that makes its module panic, with its words
  for "the application failed", and one that bounds its small store
  below an add's record, with the note the refused add would have made;
  and, to the two that look for something absent, its status saying
  the page is live (examples.md, 9.2 and 9.4). It adds: a person opens the
  example and sees its notes, under the strict policy, with no trouble;
  a field of each entry kind it uses, and a label chosen from its
  select; Escape in its dialog pressing the button naming it, and a
  focus request moving the focus; its chart drawn, its fallback read by
  its name, and a bar's mark pressed through the harness, acting as the
  choice beside it does; the component growing with its words; a note
  added in one person's page showing in another's; a reload keeping a
  pending add, which the server makes once; and the closing warning met
  by a reload while an add is pending, and by none once it is made
  (examples.md, 9.2 and 9.5).
- **Its shell takes 3.3's steps,** each once: it is the reference shell
  (step 1); its stylesheet, after `skein-ui.css` (step 2); its codec,
  its notes under `/notes/`, and `/bundle/` its bundle's prefix (step
  3); the bundler's page, and no page of its own (step 4); an
  element override, a note's region as an `<article>`, and a component,
  the body's text area that grows (step 6), calling one function of a
  JavaScript snippet of at most 10 lines, so that the bundler and the
  policy carry an application's JavaScript (step 7). It serves no root
  file (step 5) and replaces no part (step 8); it takes the file picker
  when that comes (section 15).

## 15. Later

Each adds without changing what sections 3 to 14 settle, and waits for
the application that needs it.

- **The file picker,** an effect's part: asked in the turn of a person's
  act (section 11), it opens the platform's picker through an
  `<input type="file">` of its own, within the act's event, and reports
  the file's name, type and bytes within the effect's bound; refused
  when the person cancels, or past the bound, never cut (ui.md,
  section 13). It waits for its first application; the example takes it
  then (14.3).
- **Parts for other effects** (a share sheet, a clipboard write): an
  application's own code until a second application writes the same.
- **The vocabulary that waits** (ui.md, 4.2), each with its row in
  section 6 when the light path brings it.
- **Tabs of one person sharing one stream,** so that many tabs hold one
  connection, and HTTP/2: wait for HTTP/2 in skein's HTTP, or for people
  keeping many tabs open.
- **The bundle served compressed,** compressed by the bundler and served
  in its encoding to a browser that accepts it: waits for a bundle whose
  size over the network matters.
- **Graphics beyond a scene** (a canvas of hundreds of thousands of
  points, three dimensions, video) are a component of the application's
  (6.1), never a part of skein's.

## 16. Open questions

- **Other browsers than Chromium:** the browser tier drives Chromium
  alone, through skein's browser kit; another waits for a bug that shows
  only there (browser.md, section 11).
- **Media from the origin:** the closed list (4.4) has no audio or video
  type; one waits for an application serving its own media, with media's
  role (ui.md, 4.2).
- **Right-to-left languages:** what they ask of the elements,
  `skein-ui.css` and scenes, beyond `<html lang>` and the phrasebook
  (ui.md, section 19).
- **A server keeping the previous digest directory** for a while: a
  tab opened on one bundle asks for pictures and files under its digest
  directory as it shows them, which are not found once its server
  serves another (4.4), an image showing its name. Whether, and for how
  long, waits for an application serving a new bundle while people keep
  tabs open.

## 17. What stays fixed, and why

Each rule is skein's; no value an application gives the parts removes
it. A part holds each, and an application that replaces the part (3.3,
step 8) holds the rule itself; a server holds 4.4's.

| Fixed | Why | Instead |
|---|---|---|
| text never read as markup (section 6) | A node's text carries what other people, agents and systems wrote; read as markup, it would run as the person, or restyle the page to read its text out. | Markdown, read into runs by skein-ui (ui.md, section 10); a component building its elements from its node's data (6.1) |
| an element override keeping the node's role and name (6.1) | A person reads the page with a screen reader as the fake person reads the tree (ui.md, 17.1): a role lost is a control a screen reader cannot name, and that the face, finding by role, cannot find. | any element, the page part adding ARIA's role where it differs; a component, for what no element gives (6.1) |
| the parts' invariants (5.3) | A request that leaves before it is kept is lost or made twice after a reload; a turn inside a turn steps the state machine on a step half done; a patch applied to another tree than the one it is diffed from shows a page no build made. | its own order for the rest of the turn (5.2); the part replaced, the invariant taken with it and checked by skein's stories for a browser shell (14.2) |
| nothing inline under the strict policy (4.4) | A script that runs on the page runs as the person: it can send any exchange the application can. Admitting inline script admits any script injected into the page, and styles injected inline would read the page's text out (by attribute selectors, by a font's ligatures). The parts and the bundle need nothing inline, so any server can send the strict policy, whatever the application. | its JavaScript, from the origin (3.3, step 7); its stylesheets; a component (6.1) |
| the closed list of a bundle's types (4.4) | With `nosniff`, a file is the type the server says; the list holds only the types a page loads, each weighed under the policy, and keeps off the origin what a browser would run or render (XML, a PDF, a plugin's), which `'self'` would admit. | any file of those types, in the digest directory's `files/`, or at a root path (3.3, step 5) |
