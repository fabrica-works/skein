# Setting up a forge

Provisional, 2026-10-20. `skein-forge-setup` makes a code forge ready
for a service. It ends with an access: the identity the service acts as,
its credentials, the repositories it may write and the hooks that
deliver to its listener. Each making is a plan, a step machine that says
each request and fails naming its step: Forgejo's reconcile and its
access per repository, and GitHub's App made by manifest, installed and
checked. Its host component runs the plans in the owner's loop.

## 1. In one page

- **A crate of step code.** Its plans depend on skein-lib, skein-json
  and `skein-forge` (forge.md, section 2), name no host, read no file and
  open no socket. Its host component asks its owner's io (6.3).
- **It ends with an access** (section 2): an identity, an API and a git
  credential, a reach, and for Forgejo a webhooks' secret and hooks. The
  component (forge-connection.md) needs these and nothing more.
- **Three modes** (section 3): connected, where a person makes the
  access; managed, where the service makes it with an authority the
  person gives; owned, where the service runs the forge and then sets up
  as managed. Only Forgejo can be owned.
- **The admin's token is kept,** by the owner, and read by the setup
  host alone, for reconcile and for access at adoption (section 3). The
  forge component never holds it.
- **Reconcile is Forgejo's plan** (section 4): a pure machine, events in
  and at most one need out, with six steps in a fixed order. Each reads
  first and writes only where the forge differs. Beside it: access to
  one repository at its adoption (4.5), connected's tokens checked
  (4.6), an owned forge's admin's token taken over (4.7), and a page of
  the repositories it reaches, for a picker (4.8).
- **GitHub's plans** (section 5): the App's manifest as a pure function,
  its conversion kept before the browser is answered, the bot's id, an
  installation accepted by its state, the App's hook and checks, an App
  a person made, pasted, and a page of an installation's repositories
  (5.9). Each state is one-time (5.7).
- **Given values, not configuration.** The owner gives the bot's login
  and email, the tokens' names, an owned forge's orgs and their owner,
  the admin's name for a failure, the listeners' addresses, the word and
  its limits (4.1, 6.3). A plan asks its host for each item.
- **Items are named, never placed** (6.4). Each has its bytes pinned, and
  the crate decodes them for the owner. The owner keeps each written
  item durably before it acts on the plan's end.
- **Two hosts** (section 6): the host component, which an owner runs in
  its protocol layer beside the forge component, and the one-shot host
  that skein-shell's `drive` runs for a command.
- **Verified** against answers and in worlds against the fake forge
  (section 7), on the facts of Forgejo in section 8 and of GitHub in
  section 10.

## 2. The result: an access

The setup ends with what the component needs (forge-connection.md,
section 3.5), and nothing more:

- **The identity** the service acts as, by the forge's id: Forgejo's
  bot, or GitHub's App's bot user, `<slug>[bot]`.
- **Its API credential and its git credential,** of the forge's kind
  (forge.md, section 8). Forgejo's are tokens that do not expire, with
  the scopes of 4.3, step 3, both the owner's. GitHub's is the App's id
  and private key; the component mints each installation's tokens from
  it, git's among them (forge-connection.md, section 5).
- **Its reach:** the repositories it may write. Forgejo's is the bot a
  collaborator with `write` on each repository the owner adopted (4.5).
  GitHub's is the App's installations the owner accepted (5.4).
- **Forgejo's webhooks' secret, and hooks** that deliver to the
  service's listener (forge-connection.md, section 6), one on each
  adopted repository, at `/hooks/<number>`, the number being the
  owner's for the forge. GitHub's App has a hook at no listener and no
  secret kept: the component reads its delivery log (5.5).

An access is of one forge. One without hooks is whole: a forge given no
listener, or one not on loopback, gets neither hooks nor secret, and the
service polls (forge-client.md, section 4). The crate holds no access:
it gives its host each value to keep, as items (6.4), and the owner
builds the access from them.

## 3. Three modes

- **Connected:** the person runs the forge and makes the access.
  - On Forgejo, the service strictly needs the identity's API token, git
    token and id, and for webhooks their secret. The person makes a
    forge user for the service, not a site admin, with its two tokens
    (4.3, step 3) and its write access to each repository the service
    works on, as a collaborator or through a team; and optionally each
    repository's or org's webhook to the listener, with the secret and
    the events of forge.md, section 7, and Forgejo's `[webhook]
    ALLOWED_HOST_LIST` letting it reach the listener. A repository with
    no hook is polled. The two tokens are checked as one user (4.6).
  - On GitHub, the person makes an App and pastes its id and private
    key; the service checks it, names what it lacks, sets its hook and
    installs it through its own link (5.8).
- **Managed:** the person runs the forge and gives an authority: a
  Forgejo site admin's token, or the making of a GitHub App by its
  manifest (section 5).
  - **The admin's token is kept** by the owner, durably and privately,
    and read by the setup host alone, for reconcile at each start and
    for the access at each adoption (4.5). The forge component never
    holds it, and no request of the running service carries it. It is
    asked for once, never at each adoption, and stays after setup.
  - **Forgetting it** makes the forge connected: the bot, its tokens and
    its id stay, and no reconcile runs.
- **Owned:** the service runs Forgejo on loopback and CI's runner beside
  it, makes an admin on it, and sets up as managed with that admin's
  token, which it takes over from the preparation (4.7). The preparation
  and the running are the service's (section 9). Reconcile differs in
  one step: it is given orgs to make when absent, private and owned by
  the person whose login it is given (4.1; 4.3, step 5).
- **Layered.** Each mode makes the values the one below reads:

  ```
  owned      prepare, run   ──► a forge on loopback, an admin's token
  managed    reconcile      ──► the access's values
  connected  the service    ◄── reads them
  ```

  The service is the same in every mode. Nothing passes between the
  layers but the values and the forge's number.
- **A mode is per forge.** Owned to managed: the service stops running
  the forge, and the admin's token it took over stays valid. Managed to
  connected: the admin's token is forgotten, and the values stay as last
  written. Connected to managed: an admin's token is given, and the
  connected user is kept: reconcile is given its login as the bot's and
  takes it over when its full name carries the mark (4.3, step 2), and
  refuses it `Unmarked` otherwise. The identity never changes under a
  forge's writer (forge-client.md, section 7.1).

## 4. Forgejo's plan: reconcile

Reconcile is the pure machine for a managed or an owned Forgejo. It
decides from what it reads, and keeps no record of its own: the forge is
its record. The plans beside it (4.5 to 4.8) share its events, needs and
failures.

### 4.1 What it is given

- **Given values, not configuration,** when the owner makes it:
  - the forge's number;
  - the bot's login and email;
  - the tokens' names, one for the API and one for git;
  - the orgs to make and their owner, one optional value: when the forge
    is owned, the person's orgs, at most the limit's, each made when
    absent and owned by the person's login; none when managed;
  - the repositories adopted, by id, at most `repositories` (4.5);
  - the name a failure gives the admin;
  - the listener's address, if any. The hook's URL is derived from it:
    `http://<address>/hooks/<number>`, the number being the forge's
    (4.5);
  - the word, which marks the bot;
  - its limits (4.4).

  The first owner's are temper's: the word its own name, the tokens
  `<word>-api` and `<word>-git`, the admin's name `<word>-admin`, and 8
  orgs, 120 s, 100 ms and 5 s.
- **Asked of its host, as needs,** when a step reaches them: the admin's
  token, the API and git tokens, the bot's id as last kept, and the
  webhooks' secret. These are the items (6.4). Each is named by what it
  is, never by where it lives. The host gives it, or says it has none;
  an empty value is none, for every item. The admin's token is asked for
  like the rest and held to the run's end.
- **The API's address and transport are its host's:** reconcile says a
  request, never an address. Plaintext to loopback (section 9).

### 4.2 Events in, needs out

`step(machine, event, out)` takes one event and pushes at most one need.
The host reserves room for one before each step (`MAX_OUT` is 1;
programming-model.md, section 5.2). Every plan, Forgejo's and GitHub's,
has this shape.

| Need | The host's event |
|---|---|
| `Send { request, auth }` | `Reply`: a document, a page of rows, nothing, a status, unsigned, or down |
| `Read(item)` | `Read`: the value kept, or none |
| `Write { item, bytes }` | `Written`, once the value is kept whole |
| `Draw(n)` | `Drawn`: `n` random bytes |
| `Done(outcome)`, `Stop(failure)` | none: the plan has ended |

- **The first event is `Begin`.** An event a step did not ask for ends
  the plan with `Unexpected`; a machine that has ended ignores events.
- **A request carries no credential.** Its `auth` travels beside it, and
  the host puts it in the header: `Token(secret)`, a Forgejo token sent
  as `token <value>`; `App { id, key }`, an App's JWT, which the host
  signs afresh for each request (5.3) and sends as `Bearer <jwt>`;
  `Bearer(secret)`, an installation's token sent as `Bearer <value>`
  (5.9); or `None`. A key the host cannot sign with answers `Unsigned`,
  and the request is never sent.
- **The host decodes a reply** with `skein-forge`'s decoders (forge.md,
  section 5; forge-github.md, sections 8 and 11): 2xx with a document,
  `Document`; with a listing, `Rows`, one page; 2xx with no body,
  `Empty`; any other status, `Status(code)`; no answer or a body that
  does not decode, `Down`.
- **The outcome** is `Done` for most plans, and carries what the owner
  serves or shows: the manifest's state and page (5.1), the install
  link (5.4), what an App lacks (5.6), or a page of the repositories a
  picker shows (4.8).

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
     password nobody keeps, and `must_change_password` false, since
     nobody signs in to change it. The user is never a site admin, and
     its full name carries the mark, `<word>:bot`, set by the request
     that makes it, so no run cut short leaves the bot unmarked.
   - Present: a full name without the mark stops the run with
     `Unmarked`, so a login written wrong never takes over a person's
     account; a marked user that is a site admin stops it with
     `BotIsAdmin`. The mark is found whole: at the name's start or after
     a byte outside `[a-z0-9-]`, and at its end or before such a byte,
     so the word `b` is not found in `ab:bot`.
   - A bot that passes is never edited: its email is set at its making.
3. **Its two tokens,** the API's, then git's. It asks for the one kept,
   which stays when `GET /api/v1/user` with it answers as the bot.
   - Otherwise (none kept, a 401 or a 403, or another user's answer) it
     is made again by the admin's token alone: the bot's tokens are listed
     (`GET /api/v1/admin/users/{login}/tokens`, paged) for the one of
     the token's name, which is deleted (`DELETE .../tokens/{id}`; a 404
     is as good); then made again (`POST` to the list's path) and the
     value in the answer is written to the host. A 403 is a token
     without `read:user`, as one made for git alone is. Any other
     status stops the run (`Refused`), and so does a `Down` (4.4).
   - The scopes: `read:user`, `write:issue` and `write:repository` for
     the API token; `write:repository` and `read:user` for git's, so
     that this step can ask whose it is.
   - It never uses the bot's password and never signs in as the bot.
4. **The bot's id.** It asks for the id kept, and writes it (6.4) when
   it differs.
5. **Each org to make,** in the order given, when the forge is owned:
   an absent org (404) is made (`POST
   /api/v1/admin/users/{owner}/orgs`), private and owned by the orgs'
   owner. A present one is left as it is. No team is made: the bot's
   reach is each adopted repository's (step 6). With no orgs given, the
   step reads nothing.
6. **Each adopted repository's access,** in the order given: the bot a
   collaborator, and its hook when the listener is on loopback, checked
   and repaired as 4.5 says. With none adopted, the run ends `Done`
   after step 5.

### 4.4 Converging, bounds and failures

- **From Forgejo, every time.** A second run with nothing changed
  writes nothing to the host and only the hooks' patches to the forge.
  A bot deleted, a token revoked, a hook deleted or a collaborator
  removed is made again at the next run.
- **Bounded:** the orgs at most `orgs`, the repositories at most
  `repositories`; every listing paged by the forge's page cap, read once
  from `GET /api/v1/settings/api` (`max_response_items`; zero is
  `Unexpected`), a page shorter than the cap being the last; the run
  within `deadline`. The limits are `orgs`, `repositories`, `reachable`,
  `deadline`, `backoff` and `backoff_max`, each at least 1, with
  `backoff <= backoff_max < deadline`; the crate names the pair that
  breaks it.
- **It waits for the forge.** The host makes the first request again on
  a `Down` or a 5xx, after `backoff`, doubling, never past `backoff_max`
  nor the time `deadline` leaves (6.2, 6.3), so a host may start
  reconcile beside Forgejo with no readiness probe. After the first
  answer a `Down` stops the machine.
- **A failure** is `{forge, step, subject, why}`, and shows no secret
  (lib.md, section 6). The step is `Admin`, `Bot` (its id included),
  `Token`, `Org`, `Repository` (its collaborator and hook), and for
  GitHub's plans `App`, `Installation` (a picker's too, 4.8) and `State`
  (section 5). The subject is the admin's given name, the bot's login,
  the token's name (`api` or `git` when connected, 4.6), the org's name,
  the repository's path, or its id in decimal before its path is read,
  the App's slug or id, an installation's id, or the plan whose state it
  is. The why is `Missing`, `Refused(status)`, `NotAdmin`, `BotIsAdmin`,
  `Unmarked`, `OtherUser`, `OtherApp`, `KeyForm`, `Unknown`, `Busy`,
  `Down` or `Unexpected`. It stops the run and leaves each value written
  whole and valid.
- **Several services on one forge** each have their own word and bot,
  so their own collaborators and hooks; another's bot is `Unmarked`.
- **What it cannot set:** Forgejo's settings its API does not change,
  the webhooks' `ALLOWED_HOST_LIST` and whether Actions are enabled. A
  forge whose host list refuses the listener delivers no hook, and
  polling keeps up.

### 4.5 Access to a repository, at adoption and at each start

`Adopt { forge, repository }` gives the bot access to one repository
when the owner adopts it, by the admin's token, before the owner's
first call to it; reconcile's step 6 runs the same for each adopted
repository at each start, so access a person removed is repaired.

1. **The admin's token,** as step 1 (4.3). Reconcile's run has it
   already.
2. **The repository,** `GET /api/v1/repositories/{id}`: its path. A
   `404` stops `Adopt` with `Missing`; reconcile passes over a
   repository the forge no longer has, which the owner's client finds
   gone.
3. **The bot a collaborator:** `GET
   /api/v1/repos/{owner}/{repo}/collaborators/{bot}/permission`; below
   `write`, `PUT .../collaborators/{bot}` with `{"permission":"write"}`.
   A repository the bot owns, or one whose org gives the bot more, is
   left as it is.
4. **Its hook,** when the listener is on loopback; with none, or one
   elsewhere, the plan ends `Done` after step 3.
   - The URL is `http://<address>/hooks/<number>`, the address being
     `127.x.y.z:port` or `[::1]:port`.
   - The secret is asked for once. When the host has none, the plan
     draws 32 bytes, renders 64 lowercase hex digits and writes them.
   - The repository's hooks are listed (`GET .../hooks`, paged). Of
     those with that URL the lowest id is kept and each other is deleted
     (a 404 is as good), since each would deliver every change again. A
     hook of another URL is left alone.
   - The kept hook is patched at every run, or made when none is kept:
     the URL, the secret, the subscribed events (forge.md, section 7),
     JSON and active. Forgejo never shows a hook's secret.

A collaborator or a hook on a listing's second page is found there,
neither repeated nor passed over.

### 4.6 Connected's two tokens, checked

`Check { forge }` runs when a person connects a Forgejo, and when they
replace its tokens. It asks for the API token and git's, as the person
gave them (the owner serves them from what the request carried), and
the user kept, if any.

- `GET /api/v1/user` with the API token answers the identity: its id
  and login, which the owner shows ("connected as temper, id 7"). One
  that is a site admin is refused `BotIsAdmin`: the service acts as no
  admin.
- `GET /api/v1/user` with git's must answer the same user. Either token
  refused (a 401, or git's 403 for a missing `read:user`) stops the plan
  `Refused(status)`; two users stop it `OtherUser`, each naming which
  token, `api` or `git`.
- A user kept that differs stops it `OtherUser`: a replacement answers as
  the user the forge already has. Otherwise the plan writes the user's id
  and ends `Done`; the owner keeps the tokens and the secret it was given.

### 4.7 An owned forge's admin's token, taken over

`TakeOver { forge, admin_token }` takes the token the owned forge's
preparation left in its handover file, at the service's start. The
owner reads the file and hands its value; the plan:

1. checks it as step 1 (4.3), naming the admin;
2. writes it as the admin's token;
3. lists the admin's own tokens (`GET
   /api/v1/admin/users/{admin}/tokens`, paged, the login from step 1),
   and deletes each but the one whose last eight characters are the
   taken token's, by step 3's routes (4.3), so no older admin's token
   stays valid.

The owner removes the file after `Done`, never before, and then runs
reconcile as for a managed forge. Each preparation makes a new token;
a start with no file reads the token kept.

### 4.8 The repositories a picker shows

`Reachable { forge, installation, page }` reads one page of the
repositories a forge reaches, for the owner's picker: the person picks
among them the ones to adopt (4.5). It writes nothing, to the forge or
to its host, so it runs beside the forge's one writing plan (6.3).
Every picker's read is this plan's; the forge client has none.

- **`installation`** is an installation's id on GitHub (5.9), and none
  on Forgejo. One given where the forge's kind takes none, or none where
  it takes one, stops the run at once, `Installation`, `Unexpected`,
  and sends nothing; the subject is the id given, or `none`.
- **On Forgejo, the token kept decides the listing:** the admin's
  token, when one is kept (managed or owned); else the API token that
  4.6 checked (connected). Forgetting the admin's token makes the forge
  connected (section 3), and its picker with it.

1. **The token.** The admin's token, read first, checked as step 1
   (4.3): `NotAdmin`. When none is kept, the API token; none of either
   is `Missing` (`Token`, `api`).
2. **The page cap,** read from `GET /api/v1/settings/api` as 4.4 says.
3. **One page,** `n` being the least of `reachable` and the cap:
   - with the admin's token,
     `GET /api/v1/repos/search?sort=id&order=asc&page=<page>&limit=<n>`.
     The answer is `{"ok": true, "data": [...]}`, the rows in `data`;
     one whose `ok` is not true is `Unexpected`. An admin's search finds
     every owner's repositories, the private ones too;
   - with the API token, `GET /api/v1/user/repos?page=<page>&limit=<n>`:
     the repositories its user owns or reaches as a collaborator or
     through an org's team, the private ones too, as section 8 records,
     each with its `permissions`. A row is kept only when
     `permissions.push` is true: the service works only where it can
     write.
4. **The outcome,** `Done(Reachable { rows, more })`: each row the
   repository's id, its owner's id and login, its name, and whether it
   is private; `more` when the page held `n` rows before any was
   dropped, so a connected forge's page may hold fewer and have more.

- **Paged by number,** from 1, since neither listing has a keyset: the
  owner asks for `page + 1` while `more`. In a stable order a repository
  made while the person pages lands on the last page; one deleted moves
  the later rows back by one, so the next page may pass one over, and
  the next listing finds it.
- **Bounded** by `reachable`, at least 1, the rows of one page, held in
  memory with the outcome.

## 5. GitHub's plans

One App per deployment, owned by the connecting account, public, made by
its manifest (5.1, 5.2) or by the person (5.8). Its requests and their
documents are forge-github.md's (section 11); the plans say which, in
what order, and what each answer decides. Every App's request
(`/app...`) carries the App's JWT (5.3); the conversion and the bot's
lookup carry none.

### 5.1 The manifest

`Manifest { forge, target }` makes the page that sends the person's
browser to GitHub to make the App. `target` is `Personal` or `Org(login)`.

- **A pure function** of the given values: the word and a random part,
  three bytes drawn, as six lowercase hex digits, the name being
  `<word>-<random>`; GitHub's web origin, `<web>`, which is
  `https://github.com` or the fake's (forge-github.md, section 11); the
  deployment's URL; the redirect and setup addresses on the owner's
  clients' listener, whole URLs; and the state, which the host draws
  (5.7).
- **The manifest:** `name`; `url`, the deployment's; `public` true;
  `redirect_url`, the redirect address; `setup_url`, the setup address,
  with `setup_on_update` true; `hook_attributes`, the URL
  `https://<name>.invalid/` and active (5.5); and `default_permissions`
  and `default_events`, the wanted ones (5.6).
- **The page:** a form that posts the manifest, as a field, to
  `<web>/settings/apps/new?state=<state>`, or for an org
  `<web>/organizations/<login>/settings/apps/new?state=<state>`,
  at most `page_bytes`. `Done(Manifest { state, page })` carries it
  and the state it was made for; the owner serves the page at its own
  address for that state, and keeps it no longer than the state lives
  (5.7).

The person may rename the App on GitHub's page: the slug the conversion
answers is the App's, whatever the manifest said.

### 5.2 The conversion

`Convert { forge, code, state }` runs when GitHub redirects the
person's browser back with the code and the state. The host checks the
state first (5.7). Then:

1. **The conversion,** `POST /app-manifests/{code}/conversions`, no
   credential, within the code's hour. It answers the App's id, slug,
   owner, private key as PEM and webhook secret, and an OAuth client's
   id and secret. The plan keeps none of the OAuth pair, nor the webhook
   secret (5.5).
2. **The key, turned into DER:** the PEM's `RSA PRIVATE KEY` block, its
   base64 decoded, at most `key_bytes`. Any other block, a key past
   `key_bytes`, or base64 that does not decode stops it `KeyForm`.
3. **The App's read,** `GET /app` on the JWT, proving the key signs:
   the id answered must be the conversion's (`OtherApp` otherwise), and
   an `Unsigned` stops it `KeyForm`.
4. **The bot's id** (5.3).
5. **The writes:** the App's id, slug, key (DER) and bot's id. `Done`
   comes after the last is `Written`.

The owner answers the browser only after `Done`, so a person never sees
an App made whose key the service lost, and then connects the forge
(forge-connection.md, section 3.5) and offers the install link (5.4). A
code used twice, or past its hour, is refused by GitHub: `Refused(404)`
or the status it answers.

### 5.3 The bot's id, and the JWT

- **The bot** is the App's user, `<slug>[bot]`: `GET
  /users/<slug>%5Bbot%5D`, with no credential, answers its id. A `404`
  stops the plan `Missing`. The id is the access's user, which the
  component checks at each start on an installation's token
  (forge-connection.md, section 3.4).
- **The JWT** is the dialect's `github::jwt` (forge-github.md, section
  8.1), signed by the host with `skein_tls::sign_rs256` (tls.md, section
  4) on the wall's time, issued a minute in the past and valid ten
  minutes. The host signs one for each request; the plan never sees one.

### 5.4 Installing, and accepting

- **`Install { forge }`** answers the link that installs the App, with
  a fresh state: `<web>/apps/<slug>/installations/new?state=<state>`.
  It reads the App's slug and sends nothing. Installing on another
  account is the same plan again. Which repositories the installation
  takes is the person's choice on GitHub's page; ones granted later
  arrive as its repositories' change (forge-connection.md, section 4.4).
- **`Accept { forge, installation, state }`** runs when GitHub's setup
  address brings the person back with the installation's id and the
  state. The host checks the state as one of `Install`'s (5.7). The plan
  reads the App's id and key and the accepted set, reads the
  installation (`GET /app/installations/{id}`; a `404` stops it
  `Missing`), adds its id and writes the set. The owner then hands the
  set down (forge-connection.md, section 4.4).
- **Only installations accepted this way are the service's.** Another
  account may install a public App: such an installation is never used,
  and reconcile uninstalls it (5.6).

### 5.5 The hook, at no listener

The App's hook is at `https://<name>.invalid/` at its making, and at
`https://<slug>.invalid/` once reconcile or a paste sets it, active,
with no secret the service keeps. GitHub cannot deliver there, and keeps
each delivery in the App's log, which the component reads on the App's
JWT (forge-connection.md, section 6.5). So a deployment needs no address
GitHub can reach, and no webhooks' secret is kept for GitHub: an access
that carries one is refused (forge-connection.md, section 3.5).

### 5.6 The App's checks: GitHub's reconcile

`Reconcile { forge }` runs at each start, before the owner connects the
forge, and after a connect. It reads the App's id and key and the
accepted set; then:

1. **The App,** `GET /app`: a `401` is the key no longer taken, and
   stops it `Refused(401)`, which the owner shows with the offer to
   replace the key (5.8); another id stops it `OtherApp`.
2. **Its permissions and events** against the wanted: contents, pull
   requests and issues, and merge queues, to write; checks, statuses,
   actions, administration and metadata to read; never workflows, never
   administration to write. The events: push, pull request and its
   reviews and review comments, issue comments, issues, check runs and
   suites, statuses, workflow jobs and runs, branch creation and
   deletion, and merge groups. Each lacking one is named in the outcome,
   `Lacking`, at most the wanted's count, never refused: GitHub asks the
   person to approve what a newer service asks more, on the App's page.
3. **Its hook,** `GET /app/hook/config`; one whose URL is not
   `https://<slug>.invalid/` is set to it (`PATCH /app/hook/config`).
4. **Its installations,** `GET /app/installations`, paged: an accepted
   id gone is dropped from the set, which is written; one not accepted,
   made more than `state_life` ago, is uninstalled (`DELETE
   /app/installations/{id}`; a `404` is as good). A younger one may be
   waiting for its person's `Accept`.

A second run with nothing changed writes nothing.

### 5.7 States, one-time

- **The host draws each state:** 16 bytes, as 32 lowercase hex digits,
  for `Manifest` and `Install`, bound to the forge and to the plan that
  will take it, `Convert` or `Accept`.
- **It lives `state_life`,** at most an hour, the conversion code's
  life, in the host's memory alone: at most `states`, the oldest going
  first when full. A restart forgets them; the person starts again.
  The owner keeps a manifest's page (5.1) only while its state lives,
  so at most `states` pages of `page_bytes` each.
- **Taken once:** `Convert` or `Accept` with a state that is unknown,
  used, expired, of another forge or of the other plan stops at once,
  `State`, `Unknown`, and sends nothing. The state is used when its plan
  starts, whatever its end.

### 5.8 An App a person made, pasted

`Paste { forge, app }` connects an App the person made, with its id
and private key pasted. The owner serves the pasted key as `Pasted`
(6.4), read once and never written.

1. **The key:** the pasted PEM turned into DER, as 5.2's step 2.
2. **The App's read,** `GET /app`: its id must be `app`, and the id
   kept, when the forge has one, must be too (`OtherApp`); its slug is
   read.
3. **The bot's id** (5.3); a forge that has a user kept must find the
   same (`OtherUser`).
4. **Its hook** set, and **its permissions and events** checked, as
   5.6's steps 2 and 3.
5. **The writes:** the App's id, slug, key (DER) and bot's id, then
   `Done`, carrying what it lacks.

The person installs it through `Install` (5.4). Replacing the key is the
same plan, with the id kept.

### 5.9 The repositories an installation reaches

`Reachable { forge, installation: Some(id), page }` (4.8) reads one page
of the repositories an installation of the App reaches, for the owner's
picker.

1. **The App:** its id and key, as 5.3, and `Accepted`. An id not in
   `Accepted` stops the run `Installation`, `Missing`: a picker shows
   only an installation the service accepted (5.4).
2. **The installation's token,** minted on the JWT, full
   (forge-github.md, 8.2). A `404` is `Missing`; another status is
   `Refused(status)`.
3. **One page:**
   `GET /installation/repositories?per_page=<n>&page=<page>`, on that
   token as `Bearer(secret)` (4.2), `n` being the least of `reachable`
   and 100: each row's `id`, `owner.id`, `owner.login`, `name` and
   `private` (`github::Reachable`, forge-github.md, 8.3).
4. **The outcome,** `Done(Reachable { rows, more })`, as 4.8's; `more`
   when the page holds `n` rows.

- **The token is dropped** when the page answers: never an item, never
  in an outcome, a failure or the trace, never kept past the run nor
  handed out. The next page mints again: one mint per page, accepted,
  since a picker is rare.
- **Paged and bounded** as 4.8 says, from page 1, by `reachable`.
- **Live,** no plan of this section runs (section 7): the dialect's
  conformance reads an installation's repositories on the probe App
  (section 10), and revokes each token it mints.

## 6. The crate, and its host

### 6.1 The crate

- **Step code,** under the Rust subset (programming-model.md, section
  10): the access's types and checks, Forgejo's plans, GitHub's plans,
  the items' decoders and the host component. Its plans depend on
  skein-lib, skein-json and `skein-forge`, which holds the setup's
  documents (forge.md, section 5; forge-github.md, section 11). The host
  component adds io's vocabulary, skein-http's client and skein-tls's
  client and signing, as the forge component does (forge-connection.md,
  section 2).
- **The owner checks the setup's values and limits at its start,** for
  every command it runs. The crate gives the checks. A refusal names a
  `Place`, the crate's name for each given value and each limit (an org
  or a repository by its index), which the owner maps to its own path
  for the person; never a value's content. Two limits out of order name
  both places, in the order 4.4 writes them (`backoff <= backoff_max <
  deadline`).

### 6.2 The one-shot host

The one-shot host is the service's, for a command: ordinary Rust on
skein-http over skein-io, running one plan for one forge, driven by
skein-shell's `drive` (shell.md, section 12). The plan says; the host
runs and hands back the outcome. Its duties:

- **Run a `Send`:** encode the request with `skein-forge`'s encoder, add
  the `Host` (`skein-forge`'s `host`, forge.md, section 5.1), the
  header of its `auth` and the owner's user agent, make one exchange on
  a connection closed after it, and decode the answer into a `Reply`
  (4.2).
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

### 6.3 The host component

The host component runs the plans inside a running service: a component
of the owner's protocol layer, beside the forge component and apart from
it, in skein's component shape. It holds the admin's token and the App's
key only while a plan that asked for them runs, and drops them at its
end.

- **Construction:** `Host::new(limits, settings)` checks both (6.1).
  The settings name each forge, its number, kind, API address and
  transport as forge-connection.md's do, and its given values: Forgejo's
  of 4.1 but the repositories, and GitHub's of 5.1 but the state; and
  the owner's user agent.
- **Entry points:** `down` takes one request of the owner, `up` one of
  io's events, `fire` one expired deadline, and `next_deadline` says the
  earliest; each emits at most 4 events up and 4 requests to io. The
  owner gives io its own tokens for the host's connections, keeping the
  host's token in each, as for the forge component.

| Down | Up |
|---|---|
| `Request::Run { run, plan }` | `Event::Need { run, need }`: a `Read` or a `Write` |
| `Request::Answer { run, answer }`: `Read(value)` or `Written` | `Event::Done { run, outcome }` |
| `Request::Close` | `Event::Stop { run, failure }` |
| | `Event::Closed` |

- **A run** is the owner's number for one plan:
  `Reconcile { forge, adopted }`, `Adopt { forge, repository }`,
  `Check { forge }`, `TakeOver { forge, admin_token }`,
  `Manifest { forge, target }`, `Convert { forge, code, state }`,
  `Install { forge }`, `Accept { forge, installation, state }`,
  `Reconcile { forge }` (GitHub's) or `Paste { forge, app }`, each of
  the forge's kind, or `Reachable { forge, installation, page }` (4.8,
  5.9). A forge has at most one writing run at once and one
  `Reachable` beside it: another of
  either stops at once, `Busy`, and so does one past `runs`, which
  counts both.
- **It does the needs** of 4.2 but the items: a `Send` on a connection of
  its own to the forge, opened for it and closed after its answer,
  waiting for the forge as 4.4 says on its own deadlines; a `Draw` from
  the env's source; the states of 5.7. A `Read` or a `Write` goes up,
  and the run waits for its answer. One answer per need: an answer for a
  run with no need waiting is ignored.
- **Each run ends once,** `Done` or `Stop`, within `deadline`. `Close`
  ends every run unheard and the host says `Closed` when io has settled
  every socket.

### 6.4 The items

An item is a value a plan reads or writes, named by what it is. The
owner keeps each where it chooses, durably and privately, and hands back
the bytes it was given, unchanged. It keeps each written item before it
answers `Written`, and acts on a plan's end only after: so a browser is
answered, and a connect sent, only on what a restart would find.

| Item | Bytes |
|---|---|
| `AdminToken`, `ApiToken`, `GitToken`, `WebhookSecret` | the token's or the secret's text, at most `token_bytes` |
| `User` | the identity's forge user id: 8 bytes, little-endian, never 0 |
| `AppId` | the App's id: 8 bytes, little-endian, never 0 |
| `AppSlug` | the App's slug's text, at most `name_bytes` |
| `AppKey` | the App's key, PKCS#1 `RSAPrivateKey` DER, at most `key_bytes` |
| `Accepted` | the accepted installations' ids, 8 bytes each, little-endian, ascending and distinct, at most `installations`; empty is none |
| `Pasted` | a pasted App key's PEM, read once, never written (5.8) |

- **The crate decodes each** for the owner, which builds the access
  from them (forge-connection.md, section 3.5): one total function per
  item, from its bytes to its value, none for bytes that break the
  table.
- **An item's bytes are a stored format:** a change to them is a new
  item, never new bytes for an old one. The owner's own records, its
  forge's kind and address among them, are its own.
- **`User` was the id's decimal text** before this design, and temper
  keeps it so today, in a managed or an owned forge's `user` file, which
  temper-service reads at its start. That is the one stored-format break
  this pin makes. The crate reads and writes the 8 bytes alone; the
  owner never answers a `Read` with the old value: it converts it once,
  before its first answer, or never reads it and has a plan write the
  item afresh. It tells the two forms apart by its own record, never by
  length, since an id of eight digits is eight bytes too.

## 7. Verification

The tiers and budgets are testing-strategy.md's and skein's `testing.md`.

- **Step tests,** each plan driven by answers: each step read, then
  written only where the forge differs; the marker's refusal, and the
  mark found only whole; a site admin bot refused, and a user both
  unmarked and a site admin refused `Unmarked`; a revoked token made
  again, and one answered `403`; an empty item taken as none; the hooks
  patched; an event out of order; the hook's URL for loopback addresses
  alone; the limits' relation at its bound and one past; the admin's
  search, a full page with `more`, a short last page, and a non-admin
  refused; connected's listing on the API token, its rows lacking `push`
  dropped and `more` read before the drop; an installation given to a
  Forgejo, or none to GitHub, stopped sending nothing. For GitHub: the
  manifest's contents, field by field, for each target; the conversion's
  writes before `Done`; a key of another form refused; an id other than
  the kept one refused; each permission or event lacking named; a stray
  installation uninstalled and a young one left; a picker's installation
  not accepted, and a mint's `404`, each `Missing`, and the minted token
  in no item, outcome or failure; each item's decoder at its bounds. The
  documents are tested with `skein-forge` (forge.md, section 11;
  forge-github.md, section 14).
- **The setup world** (testing-strategy.md, 2.4), in skein's
  `tests/forge/setup`, plays the host component over a store of items
  and the fake forge's setup routes and GitHub layer (fake-forge.md,
  section 4), through HTTP bytes. It checks:
  - an empty forge converged: the bot, both tokens with their scopes,
    answering as the bot, and each adopted repository's collaborator and
    hook;
  - a token kept untouched when it answers as the bot, made again when it
    is another user's or was never made, and one found on a listing's
    second page replaced;
  - a second run writing only the hooks' patches;
  - a deleted bot, a revoked token, a removed collaborator and a deleted
    hook, each made again;
  - a collaborator or a repository's hook on a listing's second page
    neither repeated nor passed over; two hooks with the listener's URL,
    the lowest id patched and the other deleted, and a hook of another
    URL left alone;
  - an owned forge's missing org made for its owner; the secret drawn
    when missing and kept when present; a person's account under the
    bot's login refused; another service's bot refused as unmarked; two
    services keeping separate access on one forge; a listener off
    loopback making no hook and no secret;
  - connected's tokens as one user, and as two, refused naming which;
    an admin's token taken over and its older tokens deleted;
  - the repositories the admin can see, listed page by page to the end,
    another owner's private one among them, while a writing run goes on
    for the same forge; a connected user's, those it can write to alone;
    and an installation's, page by page, one mint each;
  - on GitHub: the conversion's answer kept before `Done`; a state used
    twice, or unknown, refused; an installation accepted, and a stray
    one uninstalled; the hook's address set; a pasted App lacking a
    permission named;
  - no secret in a failure's `Debug` or in the trace.
- **The worst case,** against the counting allocator (skein-heap): every
  limit at its maximum, `runs` plans running, each holding its items,
  and `states` states, metered against `worst_case`.
- **Fuzzy:** random initial forges converge, and a second run writes only
  the hooks' patches (testing-strategy.md, section 8).
- **Against a real forge,** the conformance records sections 8's and
  10's facts, and a service tests its host on the throwaway Forgejo
  (fake-forge.md, sections 7 and 6). No suite calls GitHub itself.
- **The App never changes live** (fake-forge.md, section 11, which holds
  the rule). The probe App's fixture installations are accepted by no
  state, so GitHub's reconcile (5.6) would uninstall them and set the
  App's hook. No plan of section 5 runs against github.com: a live run
  never sends `DELETE /app/installations/<id>` or
  `PATCH /app/hook/config` and changes nothing of the App; strays, the
  uninstall and the hook's set are shown on the fake alone; and a
  repository a run adds to an installation or removes from it is put
  back at the run's end.

## 8. What reconcile relies on of Forgejo

Each fact is recorded by the conformance on the throwaway Forgejo at the
pin (forge.md, section 4.3), and named by the fact's name.

- **Users and tokens, by the admin's token** (steps 1 to 3, and 4.7):
  - the admin's token reads as a site admin; another's is refused:
    `setup-the-admin-token-reads-user-as-a-site-admin`,
    `setup-a-non-admin-token-is-refused-the-admin-routes`;
  - a made user is no site admin and keeps its full name; a missing
    login is a 404:
    `setup-create-user-answers-201-with-login-email-and-full-name`,
    `setup-created-user-is-no-site-admin-and-keeps-its-full-name`,
    `setup-user-of-a-login-reads-admin-flag-and-full-name`,
    `setup-user-of-a-missing-login-is-404`;
  - tokens list by name, scopes and last eight characters, never value,
    and page; are made with their value, refused under a used name, and
    deleted:
    `setup-token-list-shows-names-and-scopes-without-values`,
    `setup-token-list-shows-the-last-eight`,
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
- **Orgs** (step 5): an org an admin makes for a user is private and
  takes `write:admin` alone; a missing org is a 404:
  `setup-org-made-for-a-user-by-an-admin-is-201-and-private`,
  `setup-org-made-for-a-user-takes-write-admin-alone`,
  `setup-org-missing-is-404`.
- **Repositories' access** (step 6, 4.5): a repository reads by id for
  the admin, a private one too; an admin's `PUT` of a collaborator
  answers 204 and grants `write`, which its permission reads back; a
  repository's loopback hook is made, JSON and active, lists its events
  and never shows its secret; two may share a URL; a patch or a deletion
  leaves the repository's others; a listing pages; `ALLOWED_HOST_LIST`
  refuses loopback unless it names the listener:
  `setup-repository-by-id-reads-its-path-for-an-admin`,
  `setup-collaborator-put-by-an-admin-answers-204-and-grants-write`,
  `setup-collaborator-permission-reads-back-write`,
  `setup-repository-hook-for-a-loopback-address-is-made`,
  `setup-repository-hook-is-json-and-active`,
  `setup-repository-hook-lists-the-events-it-was-made-with`,
  `setup-repository-hook-never-shows-its-secret`,
  `setup-repository-hook-of-one-url-can-be-made-twice`,
  `setup-repository-hook-patch-and-delete-leave-the-others`,
  `setup-repository-hook-listing-pages-by-limit`,
  `delivery-loopback-is-refused-by-default`,
  `delivery-loopback-is-allowed-when-named`.
- **The admin's search** (4.8): it lists to a site admin every owner's
  repositories, the private ones too; answers `ok` and `data`; and pages
  by `page` and `limit` in id order:
  `setup-search-by-an-admin-lists-every-owners-private-repositories`,
  `setup-search-answers-ok-and-data`,
  `setup-search-sorted-by-id-pages-by-page-and-limit`.
- **A user's repositories** (4.8, connected): `GET /user/repos` on a
  user's token lists the repositories it owns, and those it reaches as a
  collaborator or through an org's team, private ones too, each with
  `permissions.push`; and pages by `page` and `limit` in a stable order:
  `setup-user-repos-lists-owned-collaborating-and-team-repositories`,
  `setup-user-repos-rows-carry-permissions-push`,
  `setup-user-repos-pages-by-page-and-limit`.
- **Paging:** the page cap is 50 by default
  (`api-default-page-cap-50-and-30`).
- **Order:** reconcile relies on none after a delivery.

## 9. Open questions

- **Owned Forgejo's preparation:** the settings file, migrations, the
  runner's registration and upgrades' dumps. Default: it stays the
  service's, since one service runs a forge. It moves into the Forgejo
  plan when a second does.
- **Reconcile over TLS,** for a managed forge on another host. Default:
  plaintext to loopback alone, and the owner refuses a forge on TLS.
  The host component speaks TLS for GitHub, so the default is the
  owner's, not the host's.
- **A hook left at an old listener's address,** after the listener
  moves. Default: left alone; the person deletes it.
- **An App's hook made inactive by hand.** `PATCH /app/hook/config` sets
  the URL, the content type and the secret, never whether it is active,
  and an inactive hook logs no delivery. Whether the App's read shows it
  stays `unchecked` on github.com (`inactive-hook-visible`,
  forge-github.md, section 13), since a live run changes nothing of the
  App (section 7); an App made to be changed would find it. Default: the
  client's polling keeps up, and a stopped log is the person's to see.
- **A fine-grained token as a connected GitHub.** Default: later; a
  connected GitHub is an App the person made (5.8).
- **The deployment's public address** as the App's hook, for a
  deployment GitHub can reach. Default: later; every App's hook is at no
  listener (5.5).

## 10. What GitHub's plans rely on of GitHub

Each fact is recorded by the conformance against github.com, on the
probe App and repositories alone, at the API's pin (forge-github.md,
section 12), and named in its section 13. No suite makes these calls. A
live run reads the App and changes nothing of it (section 7), so a fact
that needs the App changed is shown on the fake and stays `unchecked` on
github.com; temper's draft found those first (its section 11), its Setup
probe making the probe App.

- **The manifest flow,** `unchecked` live: a manifest posted with a
  state comes back with a code and the same state; the conversion
  answers the id, slug, owner, PKCS#1 PEM key and webhook secret once,
  and refuses the code again; a hook at an `.invalid` address is taken,
  active.
- **The App's routes on its JWT:** `GET /app` answers its id, slug,
  permissions and events; `GET /app/hook/config` reads the hook's URL;
  installations list and read by id. Left `unchecked` live: a JWT of a
  revoked key is a `401`, `PATCH /app/hook/config` sets the URL, and an
  installation is deleted.
- **The bot:** `GET /users/<slug>%5Bbot%5D` answers its id with no
  credential; GraphQL's `viewer` on an installation's token answers the
  same.
- **An installation's repositories** (5.9): a full token minted on the
  JWT lists them by `per_page` and `page` at
  `GET /installation/repositories`, the private ones too, each with its
  id, its owner's id and login, its name and `private`; a mint for an
  id the App lacks is a `404`; a revoked token is refused.
- **Installing,** `unchecked` live, since it takes a person's browser:
  the install link with a state brings the person back to the setup
  address with the installation's id and the state.
- **To find:** whether an `installation_target` row reaches the log
  unsubscribed; whether an inactive hook shows in the App's read stays
  `unchecked` live (section 9).
