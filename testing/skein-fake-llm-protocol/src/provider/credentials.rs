//! Credential routing and byte comparisons (fake-llm.md, 3; llm.md, 2.6).
use super::{Config, Error};
use crate::documents;
use skein_http::Header;
use skein_llm::{Credential, Provider, Scheme};

/// Names the configured route, refusing credentials or a path it cannot serve.
pub fn route(config: &Config, credentials: &[Credential]) -> Result<Provider, Error> {
    let codex = match credentials.first() {
        Some(credential) => !credential.account_id.is_empty(),
        None => false,
    };
    for credential in credentials {
        if (!credential.account_id.is_empty() && credential.scheme == Scheme::Key)
            || (config.provider == documents::Provider::Anthropic && !credential.account_id.is_empty())
            || (config.provider == documents::Provider::OpenAi && codex == credential.account_id.is_empty())
        {
            return Err(Error::Limits);
        }
    }
    match config.provider {
        documents::Provider::Anthropic => Ok(Provider::Anthropic),
        documents::Provider::OpenAi => {
            let (route, path): (_, &[u8]) = if codex {
                (Provider::OpenAiCodex, b"/backend-api/codex/responses")
            } else {
                (Provider::OpenAi, b"/v1/responses")
            };
            if config.path.as_ref() != path || (codex && config.without_credential) {
                return Err(Error::Limits);
            }
            Ok(route)
        }
    }
}

pub(super) struct Fields<'a> {
    authorization: Option<&'a [u8]>,
    api_key: Option<&'a [u8]>,
    account: Option<&'a [u8]>,
    repeated: bool,
}

impl<'a> Fields<'a> {
    pub(super) const fn new() -> Self {
        Self { authorization: None, api_key: None, account: None, repeated: false }
    }

    pub(super) fn read(&mut self, header: &'a Header) {
        let field = if header.is(b"authorization") {
            &mut self.authorization
        } else if header.is(b"x-api-key") {
            &mut self.api_key
        } else if header.is(b"chatgpt-account-id") {
            &mut self.account
        } else {
            return;
        };
        self.repeated |= field.is_some();
        *field = Some(&header.value);
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(super) enum Auth {
    Matched(u32),
    Open,
    Unauthorized,
}

pub(super) fn authorize(route: Provider, list: &[Credential], fields: &Fields<'_>, without: bool) -> Auth {
    if fields.repeated || (fields.authorization.is_some() && fields.api_key.is_some()) {
        return Auth::Unauthorized;
    }
    if fields.authorization.is_none() && fields.api_key.is_none() && fields.account.is_none() {
        return if without { Auth::Open } else { Auth::Unauthorized };
    }
    let bearer = match fields.authorization {
        Some(value) => value.strip_prefix(b"Bearer "),
        None => None,
    };
    for (index, credential) in list.iter().enumerate() {
        let matched = match route {
            Provider::OpenAi => fields.account.is_none() && bearer == Some(credential.value.as_ref()),
            Provider::OpenAiCodex => {
                bearer == Some(credential.value.as_ref()) && fields.account == Some(credential.account_id.as_ref())
            }
            Provider::Anthropic => {
                fields.account.is_none()
                    && match credential.scheme {
                        Scheme::Key => fields.api_key == Some(credential.value.as_ref()),
                        Scheme::Bearer => bearer == Some(credential.value.as_ref()),
                    }
            }
        };
        if matched {
            return Auth::Matched(u32::try_from(index).expect("bounded credential list"));
        }
    }
    Auth::Unauthorized
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::boxed::Box;

    type Row<'a> = (Provider, &'a [Credential], Option<&'a [u8]>, Option<&'a [u8]>, Option<&'a [u8]>, bool, Auth);

    fn fields<'a>(authorization: Option<&'a [u8]>, api_key: Option<&'a [u8]>) -> Fields<'a> {
        Fields { authorization, api_key, account: None, repeated: false }
    }

    fn config(provider: documents::Provider, path: &[u8], without_credential: bool) -> Config {
        Config {
            provider,
            path: path.into(),
            without_credential,
            headers: Box::new([]),
            echo: skein_llm::openai::Echo::NONE,
            usage_fields: documents::UsageFields::ALL,
        }
    }

    fn list() -> [Credential; 2] {
        [Credential::key(b"k0".as_slice().into()), Credential::bearer(b"b1".as_slice().into())]
    }

    #[test]
    fn each_route_authorizes_its_schemes_in_their_fields() {
        let public = list();
        let codex = [
            Credential::codex(b"b0".as_slice().into(), b"a0".as_slice().into()),
            Credential::codex(b"b1".as_slice().into(), b"a1".as_slice().into()),
        ];
        let rows: &[Row<'_>] = &[
            (Provider::OpenAi, &public, Some(b"Bearer k0"), None, None, false, Auth::Matched(0)),
            (Provider::OpenAi, &public, Some(b"Bearer b1"), None, None, false, Auth::Matched(1)),
            (Provider::OpenAi, &public, None, Some(b"k0"), None, false, Auth::Unauthorized),
            (Provider::OpenAiCodex, &codex, Some(b"Bearer b1"), None, Some(b"a1"), false, Auth::Matched(1)),
            (Provider::OpenAiCodex, &codex, Some(b"Bearer b1"), None, Some(b"a0"), false, Auth::Unauthorized),
            (Provider::OpenAiCodex, &codex, Some(b"Bearer b1"), None, None, false, Auth::Unauthorized),
            (Provider::Anthropic, &public, None, Some(b"k0"), None, false, Auth::Matched(0)),
            (Provider::Anthropic, &public, Some(b"Bearer b1"), None, None, false, Auth::Matched(1)),
            (Provider::Anthropic, &public, Some(b"Bearer k0"), None, None, false, Auth::Unauthorized),
            (Provider::Anthropic, &public, None, Some(b"b1"), None, false, Auth::Unauthorized),
            (Provider::Anthropic, &public, Some(b"Bearer b1"), Some(b"k0"), None, false, Auth::Unauthorized),
            (Provider::Anthropic, &public, Some(b"Bearer b1"), None, None, true, Auth::Unauthorized),
        ];
        for (row, &(route, list, authorization, api_key, account, repeated, want)) in rows.iter().enumerate() {
            let mut fields = fields(authorization, api_key);
            fields.account = account;
            fields.repeated = repeated;
            assert_eq!(authorize(route, list, &fields, false), want, "row {row}");
        }
    }

    #[test]
    fn the_route_follows_the_credentials() {
        for (row, (provider, path, list, want)) in [
            (
                documents::Provider::Anthropic,
                b"/v1/messages".as_slice(),
                Box::<[Credential]>::from(list()),
                Provider::Anthropic,
            ),
            (
                documents::Provider::OpenAi,
                b"/v1/responses".as_slice(),
                Box::<[Credential]>::from(list()),
                Provider::OpenAi,
            ),
            (documents::Provider::OpenAi, b"/v1/responses".as_slice(), Box::<[Credential]>::from([]), Provider::OpenAi),
            (
                documents::Provider::OpenAi,
                b"/backend-api/codex/responses".as_slice(),
                Box::<[Credential]>::from([Credential::codex(b"b0".as_slice().into(), b"a0".as_slice().into())]),
                Provider::OpenAiCodex,
            ),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(route(&config(provider, path, false), &list), Ok(want), "row {row}");
        }
    }

    #[test]
    fn a_list_the_route_cannot_serve_is_refused() {
        let key_with_id =
            || Credential { scheme: Scheme::Key, value: b"k0".as_slice().into(), account_id: b"a0".as_slice().into() };
        let codex = || Credential::codex(b"b0".as_slice().into(), b"a0".as_slice().into());
        for (row, (provider, path, without, list)) in [
            (
                documents::Provider::OpenAi,
                b"/backend-api/codex/responses".as_slice(),
                false,
                Box::<[Credential]>::from([key_with_id()]),
            ),
            (documents::Provider::Anthropic, b"/v1/messages".as_slice(), false, Box::<[Credential]>::from([codex()])),
            (
                documents::Provider::OpenAi,
                b"/v1/responses".as_slice(),
                false,
                Box::<[Credential]>::from([codex(), Credential::bearer(b"b1".as_slice().into())]),
            ),
            (documents::Provider::OpenAi, b"/other".as_slice(), false, Box::<[Credential]>::from(list())),
            (
                documents::Provider::OpenAi,
                b"/backend-api/codex/responses".as_slice(),
                true,
                Box::<[Credential]>::from([codex()]),
            ),
        ]
        .into_iter()
        .enumerate()
        {
            assert_eq!(route(&config(provider, path, without), &list), Err(Error::Limits), "row {row}");
        }
    }

    #[test]
    fn no_credential_field_is_authorized_only_where_configured() {
        let list = list();
        for (route, without, want) in [
            (Provider::OpenAi, false, Auth::Unauthorized),
            (Provider::OpenAi, true, Auth::Open),
            (Provider::Anthropic, false, Auth::Unauthorized),
            (Provider::Anthropic, true, Auth::Open),
        ] {
            assert_eq!(authorize(route, &list, &fields(None, None), without), want, "{route:?} {without}");
        }
    }
}
