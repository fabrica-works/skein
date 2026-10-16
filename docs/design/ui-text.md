# The text shell

Provisional, 2026-10-15. The design of `skein-ui-text`: skein-ui's shell
for a terminal or a pipe. It prints the view's tree (ui.md, section 3)
as lines, and reads each line a person types as an act on a node, a
print, or closing. It says what its parts are, when it prints, how each
role, state and property prints, its numbers and commands, its loop, and
how it is tested. What is open is in section 9.

## 1. In one page

- **The tree as lines, a line as an act.** The renderer prints the
  whole tree as lines; the command reader turns a typed line into one of
  skein-ui's acts on a node (ui.md, section 6), a print, or closing.
- **Every application's.** It names no application: an application's
  terminal client is its state and its view, composed with this shell
  under its own host, with whatever lines it prints of its own. Its
  settings carry what is the application's: its name, the longest line
  it reads, its print interval, the region whose text the first line
  carries, and how many of its own lines may wait for the output, and
  of how many bytes each. It gives its host its worst case and its
  operations, which the host adds to its own.
- **The whole page, never a diff:** at most once per interval while the
  tree changes, and at once after the person's own command.
- **By role, not by look.** Each role prints one way; states and
  properties follow as words. Style tokens and region names print
  nothing, since nothing a person must know is said by them alone
  (ui.md, section 8); nor do shortcuts, spans, focus requests and an
  image's source (section 5).
- **Numbers for what a person acts on.** Each bound, enabled node of a
  role a person acts on gets `[n]`; `n`, or `n WORDS`, acts on it with
  the act its role takes, so the view never refuses a command's act for
  its role (ui.md, section 6).
- **Nothing in the tree drives the terminal.** A control character and
  a byte that is not UTF-8 print as U+FFFD, and a secret's value never
  prints.
- **Pure where it can be.** The renderer and the command reader are step
  code, bounded from skein-ui's limits; the terminal is ordinary Rust on
  skein's io, driven by the application's host.
- **No effect carried out.** An effect the application asks of its
  platform is answered refused when its host cannot carry it out (ui.md,
  section 13).
- **Tested by golden text,** every command and refusal, and the
  terminal on skein's simulated machine (section 8).

## 2. In skein

`crates/skein-ui-text` depends on `skein-ui`, `skein-lib` and
`skein-io`, and forbids `unsafe`. Its renderer and command reader are
step code under the subset (programming-model.md, section 10), `no_std`
with `alloc`; its terminal is ordinary Rust, as shells are
(programming-model.md, 10.2), over skein's io records.

```
a person at a terminal, or a pipe
  ▲ lines out                    ▼ lines in
skein_ui_text::Terminal          the input as a read pipe, the output as a write pipe, on skein's io
  ▲ trees, the application's lines  ▼ commands: an act on a node, print, close; the room for its lines
the application's host           its loop over skein-shell's drive: its steps, its view (ui.md, section 7)
```

It is one of skein-ui's shells (ui.md, section 16), beside the browser
parts (ui-web.md). The fake person's tree face reads the same tree, not
this shell's text (ui.md, 17.1).

## 3. Its parts

- **The renderer,** `skein_ui_text::render`: from the view's tree and the
  settings' name and status region alone, the page's lines (section 5)
  and the print's numbers (section 6). A pure function, step code under
  the subset, with no io and no clock. Its output is bounded:
  `skein_ui_text::worst_case(&Limits)` gives the most lines and bytes a
  tree within skein-ui's limits prints (ui.md, section 15), with every
  text at its most bytes, every byte the replacement that grows most,
  and every node at its deepest indentation with every state and
  property it may carry. The shell's name, which the first line prints
  when the main has no level-1 heading, is at most a node's text, so the
  same limits count it.
- **A print takes its room from its own tree.** `worst_case` is what the
  host counts in its memory (ui.md, section 15; programming-model.md,
  6.3), never what the shell reserves: neither the print (`Print::new`)
  nor the terminal (`Terminal::new`) refuses limits for it, and only the
  print's numbers, one per node, are held at the limits' nodes. Each
  print's bytes are reserved as it is made, by `worst_case`'s formula
  over that tree alone: its nodes, its deepest nesting, and its text with
  the shell's name. An application's limits hold every window full and
  every text at its most at once, so their worst case can pass a page it
  builds by orders of magnitude, and pass a u32. A print holds at most
  what a list holds, a u32 of bytes: a tree whose own room passes it
  prints, in place of the page, the shell's line `NAME: page too long to
  print`, and numbers no node.
- **The command reader,** `skein_ui_text::command`: from one line, the
  last print's numbers, the view's last tree and the longest line, one
  command (section 6): an act on a node (a press, a toggle, or a field's
  input followed, when it submits on enter, by a submit: skein-ui's
  `Act`s), print, close, nothing, or a refusal, whose line
  `skein_ui_text::refusal_line` writes. Pure, step code under the
  subset.
- **The terminal,** `skein_ui_text::Terminal`: ordinary Rust. It reads
  the input as lines, writes the output, and keeps when to print
  (section 4), on skein's io: its input adopted as a read pipe, its
  output as a write pipe (io.md, section 3). Its operations carry a tag
  of their own, so its host routes their completions to it. It knows no
  application: the host gives it each completion of its operations, each
  tree the view builds and whether the view patched since the last, and
  the lines the application prints of its own, each within the room the
  terminal gives (section 4); it gives the host each command, its
  submissions, and that room (`Terminal::room`).
- **Its settings,** `skein_ui_text::Settings`, which the application
  gives whole, with no defaults:
  - *the name* its own lines start with: the application's, at most the
    bytes skein-ui's limits give a node's text (the terminal refuses a
    longer one);
  - *the longest line* it reads: at least the longest words a person may
    write, with a number and a space before them;
  - *its print interval*, a guide (section 4);
  - *its status region*: one of the application's region names (ui.md,
    section 8), whose node's text the first line carries (section 5), or
    none;
  - *its lines*: the most of the application's own lines the terminal
    holds that the output has not taken (section 4); 0 for an
    application that prints none;
  - *its line bytes*: the most bytes of each of the application's own
    lines, without the name, `: ` and the newline the terminal adds
    (section 4); 0 for an application that prints none.
- **Its worst case and its operations,** which its host adds to its own
  (programming-model.md, 6.3), each from the settings and skein-ui's
  limits, before the terminal is made: `Terminal::worst_case`, the most
  heap the terminal holds, and `Terminal::operations`, the most
  operations it has in flight at once, which the host's ring holds
  beside its own. Each is none for settings or limits the terminal
  refuses, and none past its integer (checked arithmetic). The worst
  case counts each container as it reports itself, and reserves nothing
  (a print's room is its own tree's, above):
  - its io, under the limits the terminal gives it (`skein_io::worst_case`),
    and its queues of completions, submissions, events, commands and
    lines;
  - what those queues hold at their most: each of the application's
    lines at the settings' line bytes, with the name, `: ` and the
    newline; each of its own lines at its longest, a refusal's; each
    command's words at the longest line; a read at its most bytes;
  - the line being read, at the longest line; what the output has not
    yet taken of a line being written, at the longest of the lines; the
    print's numbers, one for each node of the limits; and the page, at
    `worst_case`'s bytes (above), which the print holds until the output
    has taken it.

## 4. When it prints

- **The whole page,** always: the view's last tree, never a diff.
- **When the tree changed** (the view emitted patches since the last
  print), at most once per print interval. The interval is a guide: a
  change within it is printed when it ends, as the tree is then, never
  lost.
- **At once after the person's own command,** once the steps that took
  it have built the tree; and at once on `p`.
- **One print at a time:** while the output has not taken the last
  print, a due print waits, then shows the tree as it is when it starts.
  A print goes to the output in pieces, each at most the terminal's
  output room: 64 KiB, or the longest line with a node's text and 64
  bytes when that is more. No write is a page's size.
- **Its own lines and the application's** (a value shown once, how to
  go on) are written before the next print of the page, in the order
  they came, each starting with the shell's name and `: `, so they are
  never taken for the page's.
- **The application's lines have a room of their own.** The terminal
  holds at most the settings' lines of the application's that the
  output has not taken, and gives the host the room left
  (`Terminal::room`): the settings' lines less those waiting, and none
  once it is closing (section 7). The host says a line only within that
  room, and of at most the settings' line bytes. A line said beyond the
  room, or longer, is the host's defect: the terminal stops, as a full
  queue does (a panic, fail-stop). What has no room waits in
  the host, which bounds it as the application's design says, holding
  the person's commands if it must (section 7).
- **Its own lines keep their room.** Beside the application's lines,
  the terminal keeps room for its own: a whole read's refusals and its
  `closed`. It asks to read only while that room holds a whole read's
  refusals, whatever the application has said, so the application's
  lines never take a refusal's room nor stop the reading.

## 5. How it prints

- **The first line** is `== TITLE == STATUS`. TITLE is the text of the
  main's first level-1 heading, or the shell's name when there is none.
  STATUS is the text of the node carrying the settings' status region,
  the last such node in preorder: each of its children's text, joined by
  `; `. With no status region set, or no node carrying it, the line ends
  after `== TITLE == `, its trailing space kept. Neither the heading nor the
  status node is printed again, and nothing in the status node is
  numbered.
- **Then every node in preorder,** by its role (ui.md, 3.1):

  | Role | Prints |
  |---|---|
  | page | its children |
  | banner, navigation, main, complementary, content info, form, search, region, figure, dialog, disclosure | named: `NAME:` and its children indented; unnamed: its children |
  | heading | `#` once per level, a space, its text |
  | paragraph, text, time | its text; a time's text is its owner's words for it (ui.md, section 9) |
  | list, tree, menu | named: `NAME:` and its items indented; unnamed: its items |
  | item, tree item | `- ` before its first line, its other lines indented |
  | table | its rows, one line each |
  | row | its cells', column headers' and row headers' texts, in order, joined by ` \| ` |
  | cell, column header, row header | its text, as its row joins it; outside a row, its text on a line of its own |
  | term | its text, then `:` |
  | description | its text, indented |
  | button, tab, menu item | `[n] NAME` |
  | link | `[n] TEXT`; a checked external link of Markdown's (ui.md, section 10): `TEXT <URL>`, with no number |
  | text field, text area | `[n] NAME: VALUE`, or `[n] NAME: (empty)`; a secret's with a value, `[n] NAME: (hidden)` |
  | choice group, tabs, log | `NAME:` and its children indented, a log's entries oldest first |
  | choice | `[n] (*) TEXT` when selected, `[n] ( ) TEXT` otherwise |
  | select | named: `NAME:` and its options indented; unnamed: its options |
  | option | `[n] (*) TEXT` when selected (chosen), `[n] ( ) TEXT` otherwise |
  | check | `[n] [x] NAME` when pressed or selected, `[n] [ ] NAME` otherwise |
  | status | `* TEXT`; a status with no text prints its name |
  | alert | `! TEXT` |
  | progress | `NAME: TEXT` |
  | code | each line of its text, indented four spaces more |
  | quote | each line of its text after `> ` |
  | separator | `--` |
  | slot | its children: the nodes spliced into it (ui.md, section 5) |
  | drawing | `NAME:` and its fallback indented (ui.md, 3.5): never drawn (ui.md, 11.4) |
  | image | its name, whatever its source: one of its owner's pictures or an address (ui.md, 3.5) |

  A disclosure prints unfolded and a menu prints its items: the text
  shell keeps no fold of its own.
- **Indented** means two spaces more than the line it belongs to.
- **States** follow a node's line as words in parentheses, in this
  order: `(disabled)`, `(expanded)`, `(busy)`, `(current)`,
  `(invalid)`, `(pressed)`, `(selected)`, `(required)`; a text field or
  a text area that submits on enter adds `(enter sends)`. A check's
  pressed or selected state, and a choice's or an option's selected
  state, is its mark instead.
- **Properties** (ui.md, 3.2):
  - *an entry kind:* after a text field's value, before its states, a
    word for each kind but text and secret: `(number)`, `(email)`,
    `(telephone)`, `(url)`, `(date, YYYY-MM-DD)`, `(time, HH:MM)`, the
    last two saying the form a value takes. A secret's value prints
    `(hidden)` whenever it is not empty, and never its bytes;
  - *a description:* on a line of its own after its node's line,
    indented. The descriptions of nodes printed within another's line (a
    row's cells, the nodes of an item's or a heading's text) follow that
    line, in preorder, at the same indentation. A node that prints no
    line of its own (the page, a table, a slot, an unnamed group) prints
    its description at its children's indentation, before them;
  - *a collection's count and an item's position:* `(P of C)` at the end
    of the first line of an item, a row or a tree item with a position,
    after everything else on that line: P its position, C the count of
    the nearest list, table or tree holding it, or `(P)` when that
    carries none;
  - *a column header's sort:* `(ascending)` or `(descending)` after its
    text, within its row;
  - *not printed:* a cell's spans, a focus request (the text shell has
    no focus), an image's source, and shortcuts, Escape among them: the
    text shell takes no keys (section 6).
- **Text** is a node's runs, in order: a code run between backticks,
  every other run as it is. Style tokens and region names are not
  printed: nothing a person must know is said by them alone (ui.md,
  section 8).
- **Nothing from the tree drives the terminal.** A control character
  (U+0000 to U+001F, U+007F to U+009F) and a byte that is not UTF-8
  print as U+FFFD, except a tab, which prints as a space, and a newline,
  which starts a new line at the same indentation. This holds for every
  text the shell prints: names, runs, values, descriptions, the status
  node's text.
- **Lines are not wrapped:** the terminal wraps them.

## 6. Numbers and commands

- **Numbered nodes:** each node of a role a person acts on (a button, a
  link with an address, a text field, a text area, a choice, a check, an
  option, a tab, a menu item) that is bound and not disabled gets `[n]`,
  from 1 in preorder, at each print; one of those roles disabled or
  bound to nothing prints `[-]` in its place. An option of a disabled
  select counts as disabled. A drawing is never numbered: every act on
  it has an ordinary node beside it (ui.md, 11.3). The shell keeps its
  last print's numbers: each number's node id, which stays the node's
  while the view's diff matches it (ui.md, section 7).
- **No keys.** A shortcut, Escape among them (ui.md, 6.1), is not a
  command: the node that names it is numbered as any bound node is, and
  a line acts on it by its number.
- **Commands,** one per line, after leading and trailing spaces are
  removed:

  | Line | Does |
  |---|---|
  | `n` | acts on node n: a button, a link, a tab, a menu item or an option is pressed (a link followed, an option chosen); a check or a choice is toggled; a text field or a text area is written empty, then submitted if it submits on enter |
  | `n WORDS` | writes WORDS, the rest of the line after the spaces that follow n, as field n's whole value, a secret's as any other's, then submits it if it submits on enter |
  | `p` | prints the page again, at once |
  | `q` | closes (section 7) |
  | an empty line | nothing |

  Each act is the one its node's role takes (ui.md, section 6), so a
  command never meets the view's refusal of an act for its role. A text
  area takes one line: a line break cannot be typed into it. The words
  are passed as they were typed, whatever the field's entry kind; their
  bounds and their form are the application's, whose view marks a
  field invalid past them.
- **Refusals,** one line each, NAME being the shell's name:

  | When | The shell prints |
  |---|---|
  | n is not a number its last print showed | `NAME: no [n] on the page`, then the page again |
  | n was shown, but its node is gone or takes no such act now (unbound, disabled, an option whose select is disabled) | `NAME: [n] is gone`, then the page again |
  | words for a node that is not a field | `NAME: [n] takes no words` |
  | a number before the first print | `NAME: no page yet` |
  | a line longer than the longest | `NAME: line too long` |
  | anything else | `NAME: commands: n, n words, p, q` |

- **The commands are skein-ui-text's,** for every application. An
  application adds to its terminal only what it prints of its own
  (section 4), never a command.

## 7. Its loop

- **One line, one command.** A line is the bytes before a `\n`, without
  a `\r` just before it. Bytes with no `\n` yet wait for it; a line
  longer than the longest is refused whole, through its `\n`. The input
  is left as it is, never raw: the terminal's own line editing makes
  each line (shell.md, 6.3), and a pipe works as well as a terminal.
- **Each iteration of the host,** in order: the application's steps; the
  view built; each whole line read made into one command and done; a
  print, when one is due; the output written. Time is the host's clock,
  read once per iteration.
- **The host's pace.** The terminal asks to read only once the host has
  taken every command it gave (`Terminal::commands`), at most a read's
  worth at once. A host that takes none holds the person's lines unread
  in the input, `q` and the input's end among them, so an application
  can make the person wait, as one whose lines have no room may (section
  4); a termination signal, which the host hears, still closes.
- **Effects.** The text shell carries out none. Each effect the
  application's steps emit that its host does not carry out with code of
  its own, the host answers with that effect's one report, refused,
  which enters the application's loop as any report does (ui.md,
  section 13). A share sheet, haptics, a camera or a file picker have no
  terminal to happen in, and the commands take no file.
- **What it cannot follow.** An address outside the application (a
  provider's sign-in page, another site) is not the terminal's to open:
  the host prints, as the application's own line (section 4), what the
  person may do instead.
- **Closing:** `q`, the end of the input, or a termination signal, which
  the host hears. The terminal starts no new print, writes what it has
  begun, and says it is closed (`NAME: closed`); the host then closes
  its application, which ends as its own design says. The application's
  lines said before are written before `closed`; from `closed` on its
  room is none, and the host says no more.

## 8. Its tests

- **Golden text** fixes the renderer and the command reader: pages kept
  as files beside the crate's step tests (`src/tests/golden/`) and
  changed only by hand, built from fixture trees through skein-ui's
  builder in the crate's own tests, with no application's view:
  - a tree with every role, every state and every property: each entry
    kind, a secret with a value and one empty, a required field, a
    description on a group, on a field, on a row's cell and on a node
    that prints no line, a collection's count with its items' positions
    and positions without a count, both sorts, a cell's spans, an image
    of each source, a node naming Escape, a focus request;
  - the first line with a status region set and its node present, set
    and absent, and not set;
  - text with control characters and bytes that are not UTF-8, in a
    name, a run, a value and a description.
- **Every command and every refusal,** with numbers kept across a print
  and renumbered at the next: an option chosen by a press, a choice and
  a check toggled, a secret's words taken as its value and never
  printed, an option of a disabled select unnumbered, and refused as
  gone once its select is disabled after the print.
- **Its bound:** the renderer's output for trees at a fixture's limits,
  every text at its most bytes and of the bytes that grow most, within
  `worst_case` and within the room its own tree gives; a print and a
  terminal made from limits whose `worst_case` passes a u32, printing a
  small tree; a tree whose own room passes a u32, printed as `NAME: page
  too long to print`, with no number acting. The terminal's worst case
  grown, by one more of the application's lines or by one more byte of
  each, by what that line or byte holds; its operations skein-io's for
  its two pipes; neither for settings it refuses.
- **The terminal** on skein's simulated machine, in `tests/ui-text`
  (testing.md, section 6): lines split and joined across reads, a line
  too long refused through its newline, prints paced and one at a time,
  the application's lines before the next print, the application's
  lines within their room while the output is held, a read's refusals
  still read and written after them, no room once closing, the input
  left unread while the host has not taken its commands, the input's
  end closing without a new print, a print begun finished at closing, a
  print longer than the output room written whole, in pieces, before the
  next, a name longer than a node's text refused, an application's line
  longer than the settings' line bytes stopping the terminal.
- **An application's terminal client** is tested in its own tiers: its
  worlds play this shell, and its end-to-end tests run lines in on a
  pipe and pages out on another (testing-strategy.md, 2.9).

## 9. Open questions

- **Its own words in another language:** its markers are symbols, but
  its states, its entry kinds' words and its refusals are English words.
- **A secret typed in the clear.** The terminal's line editing echoes
  what the person types, a secret's words among them; the shell never
  prints them again. Turning the echo off for one line would take the
  terminal's modes (shell.md, 6.3) and a command that says a secret
  comes, which the commands do not have.
- **Right-to-left text** (ui.md, 4.2): what printing it in a terminal
  asks, when the vocabulary takes it.
