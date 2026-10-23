# OAuth issuer

Provisional, 2026-10-23. `skein-oauth-issuer` is the issuer's half of
OpenID Connect's authorization-code flow, beside the client's,
`skein-oauth` (oauth.md): sans-IO, `no_std` with `alloc`, and stateless.
It checks an authorization request against a client's registration,
parses and checks a token request, signs an ID token and encodes the
token's answer, the discovery document, the key set and the userinfo
answer. Codes and access tokens, their drawing, their spending and their
lives, are its caller's: in jig, the clients' component's provider.

## 1. In one page

- **Stateless, a value and functions.** An `Issuer` is built once from
  its settings, checked, and never changes: its identifier and
  endpoints, its clients' registrations, and its signing key. Each of its
  functions takes a request's bytes or a grant and gives bytes, and the
  same inputs give the same bytes. It holds no code, token, sign-in or
  clock.
- **The caller holds what lives.** It draws each code and access token
  from its own randomness, keeps each with what the issuer gave for it
  (the `Authorization`, and the signed-in `Person`), spends a code once,
  expires both, and decides who is signed in. It serves the routes, at
  addresses of its own: the issuer knows them only as the URLs it
  publishes.
- **One flow:** the authorization code, for confidential clients, with
  PKCE S256 always and an exact redirect URI. A client authenticates at
  the token endpoint by Basic or by its secret in the body, never both.
  No refresh token, no consent, no implicit or hybrid flow, no dynamic
  registration.
- **RS256 alone,** signed by `skein_tls::sign_rs256` with the issuer's
  2048-bit key; its public half, the key set's one JWK, read from the
  same key by `skein_tls::rs256_public` (tls.md, section 4), its `kid`
  the key's thumbprint. PKCS#1 v1.5 is deterministic, so a world's trace
  does not depend on ring's blinding.
- **Independent of the client.** It shares no type or codec with
  `skein-oauth`: it writes its documents with `skein-json` and reads its
  forms itself, so that its world, the client against it (section 9),
  checks two readings of the protocol against each other.
- **Bounded.** Each input is bounded by `Limits` and refused past it;
  each output is measured before it is allocated; `worst_case` prices
  the issuer and its largest call.

## 2. The issuer and its entities

```rust
pub struct Settings {
    pub issuer: Box<[u8]>,
    pub authorization_endpoint: Box<[u8]>,
    pub token_endpoint: Box<[u8]>,
    pub userinfo_endpoint: Box<[u8]>,
    pub jwks_uri: Box<[u8]>,
    pub clients: List<Client>,
    /// PKCS#1 `RSAPrivateKey` DER of a 2048-bit key, as `skein_tls::sign_rs256` takes it.
    pub key: Secret,
}
pub struct Client {
    pub id: Box<[u8]>,
    pub secret: Secret,
    pub redirect_uri: Box<[u8]>,
}
pub struct Issuer { /* the settings, checked; the key's public half and its kid */ }

impl Issuer {
    pub fn new(settings: Settings, limits: Limits) -> Result<Issuer, Error>;
}
```

- **The issuer's identifier and its URLs** are each `https` with a host,
  or `http` on loopback (`127.0.0.1`, `[::1]` or `localhost`, with a
  port) for a replaying tier's world, as the client's endpoints are
  (oauth.md, section 1); with no fragment and no userinfo, at most
  `uri_bytes`. The identifier also has no query, and no `/` at its end
  but the one of an empty path left out: `https://temper.example.com`.
- **A client** has an id of 1 to `client_bytes` visible ASCII bytes
  (`0x21` to `0x7e`), unique among the clients; a secret of 1 to
  `client_bytes` bytes; and one redirect URI, under the URLs' rules
  above, compared byte for byte wherever a request names it. There are
  1 to `limits.clients` clients. Every client is confidential.
- **The key** is one `sign_rs256` signs with; `new` reads its public
  half with `rs256_public` and refuses a key it gives none for.
- **`Error`**, `new`'s and the encoders': `Limits` (a value past its
  bound, or the settings past `worst_case`'s arithmetic), `Address` (a
  URL off its rules), `Client` (an id or secret off its rules, a
  repeated id, no client), `Key` (a key `rs256_public` refuses),
  `Claims` (a `Person` off section 6's rules), `Sign` (`sign_rs256`
  gave none for a key it took before).
- **What a request leaves to its caller,** each kept by the caller as it
  chooses: the `Authorization` (section 4), with its code; the `Person`
  (section 6), with its code and its access token.

## 3. The discovery document and the key set

`issuer.discovery() -> Box<[u8]>`, served at
`<issuer>/.well-known/openid-configuration`, and
`issuer.key_set() -> Box<[u8]>`, served at `jwks_uri`, each
`application/json`. Neither can fail: their sizes are fixed by the
settings `new` checked. Their members, in this order, with no
whitespace:

| Member | Value |
|---|---|
| `issuer` | the identifier |
| `authorization_endpoint`, `token_endpoint`, `userinfo_endpoint`, `jwks_uri` | the settings' |
| `response_types_supported` | `["code"]` |
| `response_modes_supported` | `["query"]` |
| `grant_types_supported` | `["authorization_code"]` |
| `subject_types_supported` | `["public"]` |
| `id_token_signing_alg_values_supported` | `["RS256"]` |
| `scopes_supported` | `["openid","profile","email"]` |
| `token_endpoint_auth_methods_supported` | `["client_secret_basic","client_secret_post"]` |
| `code_challenge_methods_supported` | `["S256"]` |
| `claims_supported` | `["sub","iss","aud","exp","iat","nonce","preferred_username","email","email_verified","name"]` |

The key set is `{"keys":[JWK]}` with one JWK, its members in this order:
`kty` `"RSA"`, `use` `"sig"`, `alg` `"RS256"`, `kid`, `n`, `e`.

- **`n` and `e`** are the key's modulus and public exponent as
  `rs256_public` gives them, big-endian unsigned with no leading zero
  byte (the modulus's 256 bytes; the exponent's 1 to 5, `AQAB` for
  65537), each in base64url without padding (RFC 7518, 6.3.1).
- **`kid`** is the key's JWK thumbprint (RFC 7638): base64url without
  padding of the SHA-256 (`skein_lib::sha256`) of
  `{"e":"<e>","kty":"RSA","n":"<n>"}`, those bytes exactly, 43
  characters. It names the key, so a key replaced names itself anew,
  and needs no setting.

## 4. The authorization

```rust
impl Issuer {
    pub fn authorize(&self, query: &[u8]) -> Result<Authorization, Refusal>;
    pub fn redirect(&self, authorization: &Authorization, code: &Secret) -> Box<[u8]>;
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Authorization {
    pub client: Box<[u8]>,
    pub redirect_uri: Box<[u8]>,
    pub state: Option<Box<[u8]>>,
    pub nonce: Option<Box<[u8]>>,
    /// The PKCE S256 challenge, base64url.
    pub challenge: [u8; 43],
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Refusal {
    /// Never sent back: the caller's page says so, and goes nowhere.
    Malformed,
    UnknownClient,
    UnknownRedirect,
    /// Sent back: the address to go to, the client's redirect URI with the error.
    Back { location: Box<[u8]> },
}
```

`query` is the authorization request's query, the bytes after `?`, as
the browser sent them. It is read as a form (section 5's reading) and
checked in this order, the first failure answering:

1. **Malformed,** never sent back: past `request_bytes`; a bad `%`
   escape; `client_id` or `redirect_uri` given twice.
2. **UnknownClient:** `client_id` absent, or no client's id.
3. **UnknownRedirect:** `redirect_uri` absent, or not byte for byte that
   client's. From here on, a refusal is sent back to it.
4. `invalid_request`: any other parameter given twice; `response_type`
   absent; `state` or `nonce` past `state_bytes`; `code_challenge_method`
   anything but `S256`, absent included; `code_challenge` not 43
   base64url characters.
5. `unsupported_response_type`: `response_type` anything but `code`.
6. `invalid_scope`: `scope`, split at spaces, holding no `openid`. Its
   other values are not read: `profile` and `email` change no claim
   (section 6), and others are ignored (OpenID Connect Core, 3.1.2.1).
7. `request_not_supported`, `request_uri_not_supported`: a `request` or
   a `request_uri` parameter.

Any other parameter (`prompt`, `max_age`, `login_hint`, `response_mode`
`query`) is ignored, and an empty value is an absent one (RFC 6749, 3.1).

- **A refusal sent back** is `Back { location }`: the redirect URI, then
  `?`, or `&` when it holds a query already, then `error=<the error>`,
  then `&state=<state>` when the request gave a state within its bound,
  percent-encoded (RFC 6749, 4.1.2.1).
- **Granted,** the caller decides the rest: whether a person is signed
  in, and which. It draws a code, keeps it with the `Authorization` and
  that person, and sends the browser to `redirect(&authorization,
  &code)`: the redirect URI, then `?` or `&` as above, `code=<code>`,
  and `&state=<state>` when there is one, percent-encoded. The issuer
  never decides a person is signed in, and never sends a browser
  anywhere a client's registration does not name.

## 5. The token request

```rust
impl Issuer {
    pub fn token_request(
        &self,
        authorization: Option<&[u8]>,
        content_type: Option<&[u8]>,
        body: &[u8],
    ) -> Result<TokenRequest, TokenError>;
    pub fn redeem(&self, request: &TokenRequest, authorization: &Authorization) -> Result<(), TokenError>;
}
pub struct TokenRequest {
    /// The client that authenticated.
    pub client: Box<[u8]>,
    /// For the caller to find its code by.
    pub code: Secret,
    /* the redirect URI and the PKCE verifier, private */
}
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum TokenError {
    InvalidRequest,
    /// `header`: the client tried the `Authorization` header.
    InvalidClient { header: bool },
    InvalidGrant,
    UnsupportedGrantType,
}
```

`authorization` and `content_type` are the request's headers of those
names, `body` its body. `token_request` parses and authenticates, in
this order, the first failure answering:

1. `InvalidRequest`: a content type whose media type is not
   `application/x-www-form-urlencoded` (in any case, parameters after a
   `;` ignored), none included; a body past `request_bytes`, with a bad
   `%` escape, or with a parameter given twice.
2. `InvalidRequest` for `grant_type` absent; `UnsupportedGrantType` for
   anything but `authorization_code`.
3. **The client:**
   - *with an `Authorization` header:* `Basic`, in any case, one space,
     then the standard base64 alphabet with padding of `<id>:<secret>`,
     each form-encoded (RFC 6749, 2.3.1), split at the first `:`; any
     other scheme or a malformed value is `InvalidClient { header: true
     }`. A `client_secret` in the body beside it is `InvalidRequest`
     (two methods, RFC 6749, 2.3), and a `client_id` in the body that
     is not the header's id, `InvalidRequest`.
   - *without one:* the body's `client_id` and `client_secret`, either
     absent `InvalidClient { header: false }`.
   - either way, an id no client has, or a secret not its client's,
     compared in constant time (`skein_lib::sha256::equal`), is
     `InvalidClient` with the same `header`.
4. `InvalidRequest`: `code`, `redirect_uri` or `code_verifier` absent; a
   verifier not 43 to 128 of RFC 7636's unreserved characters. A code
   past `token_bytes` is `InvalidGrant`: no code of the caller's is
   that long.

The caller then finds the code: one it never drew, spent or expired is
`InvalidGrant`, with no call to `redeem`. It spends the code on that
first presentation, whatever follows, so that a code is presented once.
`redeem` checks the request against the code's `Authorization`, each
failure `InvalidGrant`: its client is the one that authenticated; its
redirect URI is byte for byte the request's; and the base64url of the
SHA-256 of the request's verifier is its challenge, compared in
constant time.

**Its refusals' answers,** `error.answer() -> Answer`:

```rust
pub struct Answer { pub status: u16, pub challenge: Option<Box<[u8]>>, pub body: Box<[u8]> }
```

`400` with `{"error":"invalid_request"}`, `{"error":"invalid_grant"}` or
`{"error":"unsupported_grant_type"}`; `401` with
`{"error":"invalid_client"}`, and, where `header` is true, a challenge
`Basic` that the caller sends as `WWW-Authenticate` (RFC 6749, 5.2).
The caller sends every token endpoint's answer, its refusals' too, with
`Content-Type: application/json` and `Cache-Control: no-store`.

## 6. The ID token and the token's answer

```rust
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Person {
    pub subject: Box<[u8]>,
    pub username: Option<Box<[u8]>>,
    pub email: Option<Email>,
    pub name: Option<Box<[u8]>>,
}
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Email { pub address: Box<[u8]>, pub verified: bool }

impl Issuer {
    pub fn tokens(
        &self,
        authorization: &Authorization,
        person: &Person,
        access: &Secret,
        life: Duration,
        wall: Wall,
    ) -> Result<Box<[u8]>, Error>;
}
```

- **The person** is the caller's, as signed in when the code was drawn:
  a subject of 1 to `claim_bytes` bytes, and each other string at most
  `claim_bytes`, each valid UTF-8; else `Claims`. The subject is stable
  for the person and never reassigned (OpenID Connect Core, 2): what it
  is, is the caller's.
- **The access token** is the caller's draw, at least 128 bits of its
  randomness in base64url, 1 to `token_bytes` bytes, kept by the caller
  with the person for `life`; `life` is whole seconds, at least one.
- **The answer:** `{"access_token":"<access>","token_type":"Bearer",
  "expires_in":<life's seconds>,"id_token":"<the ID token>"}`, in that
  order: no refresh token and no `scope` (granted as asked, RFC 6749,
  5.1).
- **The ID token,** a compact JWS: the base64url, without padding, of
  the header `{"alg":"RS256","typ":"JWT","kid":"<kid>"}`, a `.`, of the
  claims, a `.`, and of `sign_rs256`'s signature over the two parts and
  the `.` between them.
- **Its claims,** in this order: `iss` the identifier; `sub` the
  person's subject; `aud` the authorization's client id, a string; `exp`
  the wall's seconds plus `life`'s; `iat` the wall's seconds; `nonce`
  the authorization's, when it had one; then, each where the person has
  it, `preferred_username`, `email`, `email_verified` (a boolean, with
  `email` alone) and `name`. They do not depend on the scopes asked:
  the issuer's clients are its deployment's own, with no consent to
  narrow.

## 7. The userinfo answer

```rust
impl Issuer {
    pub fn bearer(&self, authorization: Option<&[u8]>) -> Option<Secret>;
    pub fn userinfo(&self, person: &Person) -> Result<Box<[u8]>, Error>;
    pub fn userinfo_refused(&self) -> Answer;
}
```

- **`bearer`** reads the `Authorization` header of a `GET` to the
  userinfo endpoint: `Bearer`, in any case, one space, then 1 to
  `token_bytes` of RFC 6750's token characters; none otherwise. The
  caller finds the access token's person: one it never drew, or
  expired, is refused.
- **`userinfo`:** `{"sub":…}`, then `preferred_username`, `email`,
  `email_verified` and `name` as the ID token has them (section 6), the
  same claims for the same person, so that a client that compares the
  two `sub`s, as Forgejo's does, finds them equal.
- **`userinfo_refused`:** `401`, the challenge `Bearer
  error="invalid_token"`, an empty body (RFC 6750, 3.1).
- The caller sends both with `Cache-Control: no-store`, the answer as
  `application/json`.

## 8. Bounds and verification

```rust
pub struct Limits {
    pub clients: u32,
    /// The identifier, each endpoint, each redirect URI.
    pub uri_bytes: u32,
    /// A client id; a client secret.
    pub client_bytes: u32,
    /// An authorization's query; a token request's body; an `Authorization` header.
    pub request_bytes: u32,
    /// A state; a nonce.
    pub state_bytes: u32,
    /// A code; an access token.
    pub token_bytes: u32,
    /// Each of a person's subject, username, address and name.
    pub claim_bytes: u32,
}
pub fn worst_case(limits: &Limits) -> Option<u64>;
```

- **Refused at the entrance:** every input is measured against its
  bound before anything is copied from it, and a form is read in one
  pass, its values decoded into allocations of their final length.
- **Measured, then written:** each document, answer and token is
  measured with `skein-json`'s encoder before its one allocation.
- **`worst_case`** is, with checked arithmetic, the issuer held (its
  settings at their bounds, every client's id, secret and redirect URI,
  the key at `RS256_KEY_BYTES`, its public half and `kid`) plus the
  largest single call's allocations, its output and temporaries
  together: the token answer's, an ID token of a person at its bounds,
  in practice. A call allocates nothing that outlives it but what it
  returns.
- **Numbers** are the caller's: jig's profiles hold its provider's.

## 9. Testing

- **Step tests** (`crates/skein-oauth-issuer/src/tests/`), the issuer's
  functions fed by hand: `new` refusing each rule of section 2 alone;
  the discovery document and key set byte for byte for a fixture key,
  the `kid` RFC 7638's, `e` `AQAB`; every refusal of section 4 alone,
  in its order (a malformed query and an unknown redirect URI never
  sent back; a state echoed when within its bound and not past it; a
  redirect URI holding a query joined with `&`); every refusal of
  section 5 alone, Basic and the body each accepted, both refused, a
  Basic id form-encoded, the secret off by one byte; `redeem` failing
  for another client, another redirect URI and another verifier; RFC
  7636's Appendix B verifier and challenge; the ID token's claims with
  and without a nonce and each of the person's claims, its signature
  checked by `skein_tls::verify_rs256`; the userinfo answer and
  `bearer`'s refusals; every bound from both sides; and memory at the
  bounds within `worst_case`, by the counting allocator.
- **Its world,** `tests/oauth-issuer` (`skein-oauth-issuer-world`):
  `skein-oauth`'s real `Client` against the real `Issuer`, each the
  other's peer, as `tests/oauth` runs the client against the fake. The
  world plays the caller, as jig's provider will: a person signed in;
  codes of a minute, spent once; access tokens of five minutes; the
  routes, by URL. The client's registration is confidential, with PKCE
  (`pkce_for_confidential`), form-encoded, its OpenID terms naming the
  issuer and no key set, so the client reads the discovery document and
  then the key set. Clocks and seeds are the world's; a seed replays to
  the same trace and state digest; the issuer's and the client's memory
  are held to their worst cases. Its stories:
  - a sign-in: the client's record holds the person's subject, its ID
    token carries the person's claims, and the userinfo answer for its
    access token, which the world asks for as Forgejo would, names the
    same `sub`;
  - a token answer delivered after the ID token's `exp` past the
    client's skew: the client fails it;
  - a key set of another key (a second fixture) than the one the token
    is signed by: the client fails the signature.
- **Its keys** are published test keys, never a secret: RFC 7515's
  Appendix A.2 key, which skein-tls's tests use, and RFC 9500's
  `testRSA2048` (section 2.1) for the second, each a PKCS#1 DER
  fixture beside the tests.
- **Against a real client:** Forgejo, in temper's process tier, through
  jig's provider (temper's draft `sign-in.md`, section 8).

## 10. Open questions

- **A caller's denial,** `access_denied` sent back for a person signed
  in whom the caller refuses: none needs it while every person signed
  in is a member. A `deny(&authorization)` beside `redirect` when one
  does.
- **`prompt=none`,** an authorization a client asks be answered without
  a page: ignored, as the caller always shows its page.
- **Rotation:** one key; the key set would hold the old and the new
  while tokens signed by the old live, each named by its `kid`.
- **Refresh tokens:** none; a client that needs one signs in again.
