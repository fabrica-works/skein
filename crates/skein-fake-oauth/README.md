# skein-fake-oauth

An independent bounded OAuth issuer for worlds over `skein-oauth::Client`.
It parses the emitted authorization URL and token POSTs, keeps one-use codes
and rotating refresh tokens, and returns bounded token or error documents.
The issuer consumes a refresh token and rotates it when it handles an
accepted request, before its delayed response is delivered. Dropping that
response does not undo the rotation. The fake has no LLM or application
policy.

It works on records: over a network, the caller's `skein-http` stack frames
them, as `skein-fake-peers`' `oauth::Peer` does.

Optional terms issue client ids, retain each authorization’s loopback redirect,
observe added field names, and answer revocation from the same plan queue.
`granted()` lists the queued successful access tokens for a world’s LLM peer.
Revocation spends the refresh token when its successful answer is delivered.

OpenID terms sign each successful authorization-code exchange's ID token with
one public test RSA key. They publish metadata and a key set through `Get`,
without consuming a response plan. A scripted ID token takes precedence.
One fault changes a document, header or claim for the client's checks.
Signing uses ring's RSA blinding; its RS256 bytes depend only on the claims.
