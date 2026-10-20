# skein-fake-forge-github

GitHub's protocol layer over `skein-fake-forge-domain`, at the dialect's
pin. The contract is [fake-forge.md, section 10](../../docs/design/fake-forge.md#10-githubs-layer).

A shared `Service` owns App authentication, installations, browser pages,
repository routes and the blob request record. Each bounded `Server`
handles one HTTP connection. `skein-fake-peers::forge::Peer::github(address,
blob, limits, ..)` hosts API/browser routes and signed job logs on two
loopback listeners, sharing the connection limit and io buffers. The peer
learns both bound origins before clients request a log.

CI contexts render as `github-actions` check runs with the domain's run,
job and attempt identities. Reported statuses stay classic statuses; an
empty combined status is pending with zero contexts. Pull documents carry
`mergeable: null` during the domain's seeded head/base settle interval.
The domain fixtures default both settle bounds to two seconds.

A job-log request returns a 302 whose target names the repository, job and
attempt, signed with a seed-derived HMAC-SHA-256 key for 600 seconds. The
blob listener serves the whole plaintext domain log without authentication
and ignores Range. Tampered or expired targets return empty 403 responses;
this shape is inferred because GitHub's blob store is not modelled. Other
paths return 404.

`Config::blob` supplies the standalone blob origin; a peer replaces it with
its bound origin. `third_origin: Some(..)` points the API redirect at an
unfamiliar origin. `blob_redirect: Some(..)` makes the blob listener return
a second 302 to that origin. Origins are plaintext HTTP origins, validated
at admission with enough target and response room for the largest signed
path. The fixture defaults leave both choices unset.

`Limits::blob_records` retains recent requests, evicting the oldest when
full. Each record contains a path bounded by `Limits::http.head` and only
the presence of Authorization and Range. `Service::blob_records` and the
peer's corresponding accessor expose them. Blob record and response Debug output hides signed paths
and credentials. `worst_case` counts records, paths, origins, the signing
key and target scratch; per-connection buffers use the API's envelope.
