# Setting up a forge

Provisional, 2026-10-18. `skein-forge-setup` makes a code forge ready
for a service. It ends with an access: the identity the service acts as,
its credentials, the repositories it may write and the hooks that
deliver to its listener. Forgejo's making is reconcile, a step machine
that says each request and fails naming its step. GitHub's plan comes
later.

## 1. In one page

- **A crate of step code.** It depends on skein-lib, skein-json and
  `skein-forge` (forge.md, section 2), names no host, reads no file and
  opens no socket.
- **It ends with an access** (section 2): an identity, an API and a git
  credential, a reach, a webhooks' secret and hooks. The component
  (forge-connection.md) needs these and nothing more.
- **Three modes** (section 3): connected, where a person makes the
  access; managed, where the service makes it with an authority the
  person gives; owned, where the service runs the forge and then sets up
  as managed. Only Forgejo can be owned.
- **Reconcile is Forgejo's plan** (section 4): a pure machine, events in
  and at most one need out, with six steps in a fixed order. Each reads
  first and writes only where the forge differs.
- **Given values, not configuration.** The owner gives the bot's login
  and email, the tokens' names, the orgs and their owner, whether an
  absent org is made, the admin's name for a failure, the listener's
  address, the word and its limits (4.1). Reconcile asks its host for
  each secret.
- **It says each request; its host runs it** (section 6). The host
  decides where a value lives. Reconcile names no file, and fails naming
  its step, never a secret (4.4).
- **GitHub's plan is later** (section 5), as seams only.
- **Verified** against answers and in a world against the fake forge
  (section 7), on the facts of Forgejo in section 8.

## 2. The result: an access

The setup ends with what the component needs (forge-connection.md,
section 3), and nothing more:

- **The identity** the service acts as, by the forge's id.
- **Its API credential and its git credential,** of the forge's kind
  (forge.md, section 8). Forgejo's are tokens that do not expire, with
  the scopes of 4.3, step 3.
- **Its reach:** the repositories it may write. Forgejo's is a team on
  each org, the identity its member. GitHub's is the App's
  installations (section 5).
- **The webhooks' secret, and hooks** that deliver to the service's
  listener (forge-connection.md, section 6), at `/hooks/<number>`, the
  number being the owner's for the forge.

An access is of one forge. One without hooks is whole: a forge given no
listener, or one not on loopback, gets neither hooks nor secret, and the
service polls (forge-client.md, section 4). The crate holds no access:
it gives its host each value to keep.

## 3. Three modes

- **Connected:** the person runs the forge and makes the access. The
  service strictly needs the identity's API token, git token and id,
  and for webhooks their secret. The person makes:
  - a forge user for the service, not a site admin, with its two tokens
    (4.3, step 3) and its write access to each repository the service
    works on, as a collaborator or through a team;
  - optionally, each repository's or org's webhook to the listener, with
    the secret and the events of forge.md, section 7, and Forgejo's
    `[webhook] ALLOWED_HOST_LIST` letting it reach the listener. A
    repository with no hook is polled.
- **Managed:** the person runs the forge and gives an authority: a
  Forgejo site admin's token, or approval of a GitHub App (section 5).
  Reconcile asks for it at a run alone; the running service never has
  it. With no admin's token, a forge is connected.
- **Owned:** the service runs Forgejo on loopback and CI's runner beside
  it, makes an admin on it, and sets up as managed with that admin's
  token. The preparation and the running are the service's (section 9).
  Reconcile differs in one step: it is given to make an absent org,
  private and owned by the person whose login it is given (4.1; 4.3,
  step 5).
- **Layered.** Each mode makes the values the one below reads:

  ```
  owned      prepare, run   ──► a forge on loopback, an admin's token
  managed    reconcile      ──► the access's values
  connected  the service    ◄── reads them
  ```

  The service is the same in every mode. Nothing passes between the
  layers but the values and the forge's number.
- **A mode is per forge and changes by the owner's values and a
  restart.** Owned to managed: the service stops running the forge, and
  the admin's token it made stays valid. Managed to connected: no
  reconcile runs, and the values stay as last written. Connected to
  managed: an admin's token is given; reconcile adopts or makes the bot.

## 4. Forgejo's plan: reconcile

Reconcile is the pure machine for a managed or an owned Forgejo. It
decides from what it reads, and keeps no record of its own: the forge is
its record.

### 4.1 What it is given

- **Given values, not configuration,** when the owner makes it:
  - the forge's number;
  - the bot's login and email;
  - the tokens' names, one for the API and one for git;
  - the orgs, at most the limit's;
  - the orgs' owner: the person's login when the forge is owned, none
    when managed;
  - whether an absent org is made: yes when owned, no when managed. It
    needs the orgs' owner;
  - the name a failure gives the admin;
  - the listener's address, if any. The hook's URL is derived from it:
    `http://<address>/hooks/<number>`, the number being the forge's
    (4.3, step 6);
  - the word, which marks the bot;
  - its limits (4.4).

  The first owner's are temper's: the word its own name, the tokens
  `<word>-api` and `<word>-git`, the admin's name `<word>-admin`, and 8
  orgs, 120 s, 100 ms and 5 s.
- **Asked of its host, as needs,** when a step reaches them: the admin's
  token, the API and git tokens, the bot's id as last kept, and the
  webhooks' secret. These are the items. Each is named by what it is,
  never by where it lives. The host gives it, or says it has none. The
  admin's token is asked for like the rest and held to the run's end.
- **The API's address and transport are its host's:** reconcile says a
  request, never an address. Plaintext to loopback (section 9).

### 4.2 Events in, needs out

`step(machine, event, out)` takes one event and pushes at most one need.
The host reserves room for one before each step (`MAX_OUT` is 1;
programming-model.md, section 5.2).

| Need | The host's event |
|---|---|
| `Send { request, token }` | `Reply`: a document, a page of rows, nothing, a status, or down |
| `Read(item)` | `Read`: the value kept, or none |
| `Write { item, bytes }` | `Written`, once the value is kept whole |
| `Draw(n)` | `Drawn`: `n` random bytes |
| `Done`, `Stop(failure)` | none: the run has ended |

- **The first event is `Begin`.** An event a step did not ask for ends
  the run with `Unexpected`; a machine that has ended ignores events.
- **A request carries no credential.** The token travels beside it, and
  the host puts it in the header.
- **The host decodes a reply** with `skein-forge`'s decoders (forge.md,
  section 5): 2xx with a document, `Document`; with a listing, `Rows`,
  one page; 2xx with no body, `Empty`; any other status, `Status(code)`;
  no answer or a body that does not decode, `Down`.

### 4.3 The steps

Six steps, in this order. Each reads first and writes only where the
forge differs.

1. **The admin.** It asks for the admin's token: absent or empty stops
   the run with `Missing`. `GET /api/v1/user` with it must answer a
   site admin, or the run stops with `NotAdmin`. Either failure names
   the admin by the given name. Every later request carries the token,
   but step 3's check of a kept token.
2. **The bot.** It reads the user of the bot's login.
   - Absent (404): it draws 32 bytes and makes the user (`POST
     /api/v1/admin/users`) with them as 64 lowercase hex digits for a
     password nobody keeps. The user is never a site admin, and its full
     name carries `<word>:bot`, set by the request that makes it, so no
     run cut short leaves the bot unmarked.
   - Present: a full name without `<word>:bot` stops the run with
     `Unmarked`, so a login written wrong never takes over a person's
     account. A site admin stops it with `BotIsAdmin`.
   - A bot that passes is never edited: its email is set at its making.
3. **Its two tokens,** the API's, then git's. It asks for the one kept,
   which stays when `GET /api/v1/user` with it answers as the bot.
   - Otherwise (none kept, a 401, or another user's answer) it is made
     again by the admin's token alone: the bot's tokens are listed
     (`GET /api/v1/admin/users/{login}/tokens`, paged) for the one of
     the token's name, which is deleted (`DELETE .../tokens/{id}`; a 404
     is as good); then made again (`POST` to the list's path) and the
     value in the answer is written to the host.
   - The scopes: `read:user`, `write:issue` and `write:repository` for
     the API token; `write:repository` and `read:user` for git's, so
     that this step can ask whose it is.
   - It never uses the bot's password and never signs in as the bot.
4. **The bot's id.** It asks for the id kept, and writes it in decimal
   when it differs.
5. **Each org,** in the order given.
   - An absent org (404) is made when so given (`POST
     /api/v1/admin/users/{owner}/orgs`), private and owned by the orgs'
     owner. When not, the run stops with `OrgMissing`.
   - In it, a team named the bot's login: found in the org's teams
     (paged), made when absent, edited when it differs from the wanted
     one. The wanted team is on every repository the org has or will
     have (`includes_all_repositories`), its `permission` `write`, sent
     explicitly, and `write` on the units a service uses: code, issues,
     pull requests and actions. Adopting any repository of the org then
     finds the bot's write access with no admin act at runtime.
   - The bot its member: the team's members are listed (paged), and the
     bot is added when absent.
6. **Each org's webhook,** when the listener is on loopback. With none,
   or one elsewhere, the run ends `Done` after step 5.
   - The URL is `http://<address>/hooks/<number>`, the address being
     `127.x.y.z:port` or `[::1]:port`.
   - The secret is asked for once. When the host has none, reconcile
     draws 32 bytes, renders 64 lowercase hex digits and writes them.
   - Per org, the hooks are listed (paged). Of those with that URL the
     lowest id is kept and each other is deleted (a 404 is as good),
     since each would deliver every change again. A hook of another URL
     is left alone.
   - The kept hook is patched at every run, or made when none is kept:
     the URL, the secret, the subscribed events (forge.md, section 7),
     JSON and active. Forgejo never shows a hook's secret.

### 4.4 Converging, bounds and failures

- **From Forgejo, every time.** A second run with nothing changed
  writes nothing to the host and only the hooks' patches to the forge.
  A bot deleted, a token revoked, a hook deleted, a team member removed
  or a team a person edited is made again at the next run.
- **Bounded:** the orgs at most `orgs`; every listing paged by the
  forge's page cap, read once from `GET /api/v1/settings/api`
  (`max_response_items`; zero is `Unexpected`), a page shorter than the
  cap being the last; the run within `deadline`. The limits are `orgs`,
  `deadline`, `backoff` and `backoff_max`, each at least 1, with
  `backoff <= backoff_max < deadline`; the crate names the pair that
  breaks it.
- **It waits for the forge.** The host makes the first request again on
  a `Down` or a 5xx, after `backoff`, doubling, never past `backoff_max`
  nor the time `deadline` leaves (6.2), so a host may start reconcile
  beside Forgejo with no readiness probe. After the first answer a
  `Down` stops the machine.
- **A failure** is `{forge, step, subject, why}`, and shows no secret
  (lib.md, section 6). The step is `Admin`, `Bot` (its id included),
  `Token`, `Org` (its team included) or `Hook`. The subject is the
  admin's given name, the bot's login, the token's name or the org's
  name (the `Hook` step's too). The why is `Missing`, `Refused(status)`,
  `NotAdmin`, `BotIsAdmin`,
  `Unmarked`, `OrgMissing`, `Down` or `Unexpected`. It stops the run and
  leaves each value written whole and valid.
- **Several services on one forge** each have their own word and bot,
  so their own team and hooks; another's bot is `Unmarked`.
- **What it cannot set:** Forgejo's settings its API does not change,
  the webhooks' `ALLOWED_HOST_LIST` and whether Actions are enabled. A
  forge whose host list refuses the listener delivers no hook, and
  polling keeps up.

## 5. GitHub's plan, later

Marked later. It says what the plan will ask, and binds no code.

- **Managed is the App manifest flow:**
  1. The machine drafts the manifest: the App's name, URL, webhook URL
     and events, and its permissions (contents, issues and pull requests
     to write; checks, statuses, actions and metadata to read).
  2. The person creates the App on GitHub, which redirects their
     browser to the host's loopback listener with a code, as in
     oauth.md, section 6.5.
  3. The machine exchanges the code for the App's id, private key and
     webhook secret.
  4. The person installs the App on their orgs or repositories.
  5. The machine finds the installations and the repositories each one
     reaches.
- **Reconcile then checks and reports.** No API makes an installation,
  so it repairs nothing and names what the person must do. Installing
  the App is its scope, in place of teams.
- **The credentials are of the installation kind** (forge.md,
  section 8): the identity is the App's bot user, and git authenticates
  over HTTPS with the installation's token, which expires within the
  hour and which the component renews (forge-connection.md, section 5).
- **Connected** is a fine-grained token, or an App the person makes and
  installs. **Owned:** none.

## 6. The crate, and its host

### 6.1 The crate

- **Step code,** under the Rust subset (programming-model.md,
  section 10): the access's types and checks, Forgejo's reconcile, and
  GitHub's plan later. It depends on skein-lib, skein-json and
  `skein-forge`, which holds the setup's documents: admin users and
  their tokens, orgs, teams and members, and org hooks (forge.md,
  section 5; section 4.3).
- **The owner checks the setup's values and limits at its start,** for
  every command it runs. The crate gives the checks, and a refusal names
  the value's place, never its content.

### 6.2 The host

The host is the service's: ordinary Rust on skein-http over skein-io,
one-shot, running one reconcile for one forge, driven by skein-shell's
`drive` (shell.md, section 12). Reconcile says; the host runs and hands
back the outcome. A service with several forges runs a machine for each.
Its duties:

- **Run a `Send`:** encode the request with `skein-forge`'s encoder, add
  the `Host`, `Authorization: token <value>` and the owner's user agent,
  make one exchange on a connection closed after it, and decode the
  answer into a `Reply` (4.2).
- **Keep and give the items.** It decides where each lives, and keeps it
  private and whole: written beside its place and renamed over it. A
  `Read` is answered with the value, or none.
- **Draw** `n` bytes from its own source of randomness.
- **Wait for the forge** as 4.4 says: the waiting and its backoff are
  the host's, never the machine's. When `deadline` passes, end the run:
  before the first answer, naming the forge and the deadline; after it,
  answering the request in flight `Down`.
- **Hold the forge's lock** across a run, and say the outcome in the
  service's way: its exit status, its words.

## 7. Verification

The tiers and budgets are testing-strategy.md's and skein's `testing.md`.

- **Step tests,** reconcile driven by answers: each step read, then
  written only where the forge differs; the marker's refusal; a site
  admin bot refused; a revoked token made again; the hooks patched; an
  event out of order; the hook's URL for loopback addresses alone; the
  limits' relation at its bound and one past. The documents are tested
  with `skein-forge` (forge.md, section 11).
- **Reconcile's world** (testing-strategy.md, 2.4) plays the host over a
  store of items and the fake forge's setup routes (fake-forge.md,
  section 4). It checks:
  - an empty forge converged: the bot, both tokens with their scopes,
    answering as the bot, the team and the hooks;
  - a token kept untouched when it answers as the bot, and made again
    when it is another user's or was never made;
  - a second run writing only the hooks' patches;
  - a deleted bot, a revoked token, a removed team member, a deleted
    hook and a team a person edited, each made again;
  - two hooks with the listener's URL, the lowest id patched and the
    other deleted, and a hook of another URL left alone;
  - a missing org refused when given not to be made, and made for the
    orgs' owner when given so; a listing longer than a page read to its
    end;
  - the secret drawn when missing and kept when present; a person's
    account under the bot's login refused;
  - no secret in a failure's `Debug` or in the trace.
- **Fuzzy:** random initial forges converge, and a second run writes only
  the hooks' patches (testing-strategy.md, section 8).
- **Against a real Forgejo,** the conformance records section 8's facts,
  and a service tests its host on the throwaway (fake-forge.md,
  sections 7 and 6).

## 8. What reconcile relies on of Forgejo

Each fact is recorded by the conformance on the throwaway Forgejo at the
pin (forge.md, section 4.3), and named by the fact's name.

- **Users and tokens, by the admin's token** (steps 1 to 3):
  - the admin's token reads as a site admin; another's is refused:
    `setup-the-admin-token-reads-user-as-a-site-admin`,
    `setup-a-non-admin-token-is-refused-the-admin-routes`;
  - a made user is no site admin and keeps its full name; a missing
    login is a 404:
    `setup-create-user-answers-201-with-login-email-and-full-name`,
    `setup-created-user-is-no-site-admin-and-keeps-its-full-name`,
    `setup-user-of-a-login-reads-admin-flag-and-full-name`,
    `setup-user-of-a-missing-login-is-404`;
  - tokens list by name and scopes, never value, and page; are made
    with their value, refused under a used name, and deleted:
    `setup-token-list-shows-names-and-scopes-without-values`,
    `setup-token-list-pages-by-page-and-limit`,
    `setup-admin-route-makes-a-token-answering-201-with-name-scopes-and-value`,
    `setup-token-of-an-existing-name-is-refused`,
    `setup-token-delete-answers-204-and-ends-the-token`;
  - the scopes: the token routes need `write:admin`; `GET /user` a user
    scope; a git token `read:user`; an API token `write:issue`:
    `setup-admin-token-without-write-admin-is-refused-the-token-routes`,
    `setup-admin-token-without-a-user-scope-is-refused-user`,
    `setup-a-git-token-of-write-repository-alone-is-refused-user`,
    `setup-a-git-token-of-write-repository-and-read-user-answers-user-as-the-bot`,
    `setup-api-token-without-write-issue-is-refused-an-issue-write`.
- **Orgs and teams** (step 5): an org an admin makes for a user is
  private and takes `write:admin` alone; a missing org is a 404; a team
  on all repositories with `write` on four units reads back, pages and
  is edited whole; its member lists and writes in a later repository:
  `setup-org-made-for-a-user-by-an-admin-is-201-and-private`,
  `setup-org-made-for-a-user-takes-write-admin-alone`,
  `setup-org-missing-is-404`,
  `setup-team-made-with-write-all-repositories-and-four-units`,
  `setup-team-listing-reads-back-what-was-made`,
  `setup-team-listing-pages-by-limit`,
  `setup-team-edit-replaces-units-and-flags`,
  `setup-team-members-list-the-added-user`,
  `setup-team-member-writes-in-a-repository-made-later`.
- **Org hooks** (step 6): a loopback hook is made, JSON and active,
  lists its events and never shows its secret; two may share a URL; a
  patch or a deletion leaves the org's others; a listing pages;
  `ALLOWED_HOST_LIST` refuses loopback unless it names the listener:
  `setup-hook-for-a-loopback-address-is-made`,
  `setup-hook-is-json-and-active`,
  `setup-hook-lists-the-events-it-was-made-with`,
  `setup-hook-never-shows-its-secret`,
  `setup-hook-of-one-url-can-be-made-twice`,
  `setup-hook-patch-leaves-the-orgs-other-hook`,
  `setup-hook-delete-is-204-and-leaves-the-orgs-other-hooks`,
  `setup-hook-listing-pages-by-limit`,
  `delivery-loopback-is-refused-by-default`,
  `delivery-loopback-is-allowed-when-named`.
- **Paging:** the page cap is 50 by default
(`api-default-page-cap-50-and-30`).
  **Order:** reconcile relies on none after a delivery.

## 9. Open questions

- **Owned Forgejo's preparation:** the settings file, migrations, the
  runner's registration and upgrades' dumps. Default: it stays the
  service's, since one service runs a forge. It moves into the Forgejo
  plan when a second does.
- **Reconcile over TLS,** for a managed forge on another host. Default:
  plaintext to loopback alone, and the owner refuses a forge on TLS.
- **A hook left at an old listener's address,** after the listener
  moves. Default: left alone; the person deletes it.
- **Repositories outside the orgs,** a person's own among them.
  Default: as connected's: the person makes the bot a collaborator and
  adds the hook, or the repository is polled. Making one writable and
  hooked without the person is later.
- **GitHub's needs of its host:** a link shown, a redirect heard, a key
  kept. Default: decided with its plan.
