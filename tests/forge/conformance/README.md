# Throwaway Forgejo conformance

The Rust tool starts only the pinned Forgejo binary named by
`--forgejo`, with fresh SQLite state on loopback. It creates the
fixture and removes its instance and scratch directory when it ends.
`VERSION` and `RUNNER_VERSION` in
`skein_throwaway_forgejo` pin both binaries.

```sh
heavy --gate scripts/proof-forgejo-conformance.sh \
  --forgejo <absolute path to the pinned Forgejo> \
  --runner <absolute path to the pinned runner> \
  --scratch target/forgejo-conformance --out tests/forge/conformance
```

Scratch must be empty or absent, outside `/tmp`. The binary's first version
line and SHA-256 are kept in `transcripts/binary.txt`.

Each case is `transcripts/<case>/{request,response}`: raw HTTP/1.1 bytes.
Authorization is `token MASKED` or `Basic MASKED`. Responses are unframed, with chunking
removed and Content-Length recomputed. JSON key order, whitespace and types
remain as received. Cases replace their own directory only.

The setup group records admin user and token routes under the minimum
admin scope set. It registers newly made token values and their eight-digit
tails before masking, and checks every kept capture for those secrets.
The component facts run under `read:user`, `write:issue`, and `write:repository`.

One map masks object ids, UUIDs and quoted RFC3339 times by first appearance across the
run, preserving id width and time offset. Ports become 3000; Date and
Last-Modified become 2020-01-01. Numeric resource ids remain stable because
provisioning has a fixed order. Every output is checked for all fixture
secrets before it is written, and response bodies are also checked before
masking. A leak reports the case and secret kind without printing its value.

The same run writes `facts.txt`, with Forgejo and runner provenance followed
by `holds`, `fails` or `unchecked` lines. Pass `--runner` naming the
pinned runner for Actions facts and eleven Actions transcripts.
The runner connects through its private token file and executes two inline host
shell jobs, one succeeding and one failing; no actions or containers are
downloaded. Without a runner those facts remain unchecked. Webhook deliveries are recorded
in `deliveries/<event-type>/{request,response}` with their own mask map;
the listener holds its answer while status and mergeability probes run.

`--component` drives the forge component through real io sockets on the shell's
ring against a fresh fixture. It replaces only the `component-` facts in
`facts.txt`; it records no transcripts. The same runner flag exercises
Checks and Job through the component; without it those two facts are unchecked.
Start requests are counted separately from logical calls. Every call's cost
is compared with the HTTP request lines counted from socket sends.

The `cli-` facts check migration, users, scoped tokens and dumps through Forgejo’s command line, on a fresh work path and the running fixture. They record only outcomes, never command output or credentials.

The `runner-` facts check secret-file registration, the deprecated offline file command, and the runner’s connection through its token file. A final workflow checks cloning a private repository with the job’s token. No registration file is used by the running runner.

Response unit arrays are sorted. Git pkt-line length prefixes stay intact while
their payloads are masked. Unquoted runner log times remain in the corpus;
the mask completion step owns that repair. Delivery CI requires `--runner`;
recording refuses it when the flag is absent.
