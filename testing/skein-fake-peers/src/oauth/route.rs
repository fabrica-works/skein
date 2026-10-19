//! The issuer's configured HTTP paths and value-free authorization targets.
use skein_http::Method;

pub(super) struct Paths {
    pub(super) authorize: Box<[u8]>,
    pub(super) token: Box<[u8]>,
    pub(super) revoke: Option<Box<[u8]>>,
    pub(super) metadata: Option<Box<[u8]>>,
    pub(super) key_set: Option<Box<[u8]>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Route {
    Authorize,
    Token,
    Revoke,
    Metadata,
    KeySet,
    Missing,
}

pub(super) fn route(paths: &Paths, method: Method, target: &[u8]) -> Route {
    let base = target.split(|byte| *byte == b'?').next().expect("target has a base");
    if method == Method::Get && base == paths.authorize.as_ref() {
        Route::Authorize
    } else if method == Method::Post && target == paths.token.as_ref() {
        Route::Token
    } else if method == Method::Post && Some(target) == paths.revoke.as_deref() {
        Route::Revoke
    } else if method == Method::Get && Some(target) == paths.metadata.as_deref() {
        Route::Metadata
    } else if method == Method::Get && Some(target) == paths.key_set.as_deref() {
        Route::KeySet
    } else {
        Route::Missing
    }
}

pub(super) fn observed_target(target: &[u8]) -> Box<[u8]> {
    let Some(at) = target.iter().position(|byte| *byte == b'?') else { return target.into() };
    let mut out = target[..=at].to_vec();
    for (index, field) in target[at + 1..].split(|byte| *byte == b'&').enumerate() {
        if index != 0 {
            out.push(b'&');
        }
        let name = field.split(|byte| *byte == b'=').next().expect("query field has a name");
        if name == b"id_token_hint" {
            out.extend_from_slice(name);
        } else {
            out.extend_from_slice(field);
        }
    }
    out.into_boxed_slice()
}

#[cfg(test)]
mod tests {
    use super::{Paths, Route, route};
    use skein_http::Method;

    fn paths() -> Paths {
        Paths {
            authorize: b"/authorize".as_slice().into(),
            token: b"/token".as_slice().into(),
            revoke: Some(b"/revoke".as_slice().into()),
            metadata: Some(b"/.well-known/openid-configuration".as_slice().into()),
            key_set: Some(b"/jwks".as_slice().into()),
        }
    }

    #[test]
    fn route_names_each_endpoint() {
        for (method, target, expected) in [
            (Method::Get, &b"/authorize?state=s"[..], Route::Authorize),
            (Method::Post, b"/token", Route::Token),
            (Method::Post, b"/revoke", Route::Revoke),
            (Method::Get, b"/.well-known/openid-configuration", Route::Metadata),
            (Method::Get, b"/jwks", Route::KeySet),
            (Method::Post, b"/jwks", Route::Missing),
            (Method::Get, b"/token", Route::Missing),
            (Method::Get, b"/other", Route::Missing),
        ] {
            assert_eq!(route(&paths(), method, target), expected, "{method:?} {target:?}");
        }
    }
}
