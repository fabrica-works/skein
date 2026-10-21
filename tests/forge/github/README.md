# GitHub conformance

Run the hosted fake, with invented fixtures and no machine sockets:

```sh
heavy cargo run -p github-conformance -- --fake --out /tmp/github-conformance-out
```

Use a new output directory for each run. `facts.txt` records the named facts
and provenance; `transcripts/<case>/{request,response}` retains the masked
exchanges. Requests use the dialect's plans where it has one. Numeric resource
ids are replaced consistently by first appearance; authorization, returned
credentials, signed blob addresses, UUIDs, node ids, ETags, object ids and times
are masked. The shared Forgejo/GitHub scanner refuses unexpected secrets before
writing transcripts, facts or the journal, naming only their case and kind.

The live path is the fixture owner's alone, from the owner's session with the
director's word. Never run it from a suite, gate, implementer session or CI.
It is built and its refusals are tested offline; development never runs it.
The owner invokes `--live <directory> --out <new-directory>`. No environment
configuration is read. The directory must be 0700; every file read must be a
regular 0600 file, with symlinks refused. Other files are ignored.

`app.json` supplies the App's `id`, `slug`, `name`, `client_id`, `node_id`,
`html_url`, `permissions` (object of strings), `events` (array of strings),
`created_at`, `owner`, `installations` (array of `{id, account, repositories}`),
`spare_repository`, `hook_url`, and `api_version`. Each repository is an
`owner/name` string. Supply two or more installation accounts, each with a probe
repository and `person-<account>.token`; `app.pem` holds the RS256 RSA private
key in PEM and `webhook_secret` holds the hook secret. The first installation
reaches only its one probe repository, the second reaches all its account's
repositories, and the spare is ungranted in the first account. The probe `ci`
workflow fails on `FAIL` in the commit message. All names come from these files.
Startup reads each probe repository with its owner's person token before any
write. The first whose `owner.type` is `Organization` supplies the repository,
installation token and matching person token for all queue cases and rulesets.
At least one organization installation is required; otherwise startup stops
before minting or repository changes. No account-type fixture field is needed.
The version must equal the dialect's API pin. Tokens expiring within thirty
days are named in the facts' provenance.

Live limits repository requests to the exact fixture repository names and
allows only GET on a repository itself. User and numeric repository routes
are read only, except installation repository grants and removals for fixture
installation ids. GraphQL accepts only POST with the dialect's exact VIEWER,
PULL and ENQUEUE documents; enqueuePullRequest is its only mutation field.
App mutations, including manifest conversion, installation removal and hook
changes, are refused before the connector. Refusals name only method and path.
Its requests and redirected blob reads are bounded, TLS verified, and blob
reads carry no authorization.
The run reserves requests for cleanup and stops within 400 requests.

Every repository change and token mint is written to `made.txt` before its
request. Cleanup closes issues and pulls, deletes owned branches and rulesets,
and restores installation access. It targets generated names so a lost reply
can be recovered by lookup. `--clean <out>` resumes those intentions; live
cleanup remembers the fixture directory path, and `--live <directory>` may
supply a relocated directory. The path contains no copied credential.

Minted tokens stay in memory and are revoked on every controlled exit; after
an externally killed process, `--clean` reports each unacknowledged mint's
installation and one-hour expiry without recovering or writing its value.
Those reports appear only in `facts.txt`; a lost mint response is reported the
same way. The fake's state belongs to its process, so the command-line fake
cleanup starts from fresh state; the interruption test retains the hosted
world while reloading the same durable journal.

The fake also checks stray installation removal and setting the hook address;
these App mutations are unchecked live. Whole logged payloads are checked against
the webhook secret before their signatures and payloads enter masked transcripts.

The focused tests run every named fact on the fake, check the design's name
list, masking and leak refusal, fixture permissions, the pre-connector App
and fixture boundary, replay of the fake's requests through the guard, request
exhaustion, token expiry, interrupted cleanup and revocation on failed cases.
Required signatures remain unchecked because the fake has no
signature enforcement. Live alone leaves the two manifest/hook facts unchecked
because it changes nothing of the App.
