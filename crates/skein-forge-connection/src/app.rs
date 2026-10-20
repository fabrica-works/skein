//! The App's signed token cache (forge-connection.md, 5.2).

use skein_forge::github::jwt;
use skein_lib::{Duration, Secret, Wall};

/// One GitHub forge's App JSON Web Token, signed and reused (5.2).
#[derive(Debug)]
pub(crate) struct Jwt {
    token: Option<Secret>,
    expires: Wall,
}

impl Jwt {
    /// Holds no token.
    pub(crate) fn new() -> Self {
        Self { token: None, expires: Wall::EPOCH }
    }

    /// The token to send now, or `None` when the key will not sign.
    pub(crate) fn bearer(&mut self, app: u64, key: &Secret, wall: Wall, deadline: Duration) -> Option<&Secret> {
        let fresh = self.fresh(wall, deadline);
        if !fresh {
            self.forget();
            let issued = wall.as_nanos().saturating_sub(Duration::from_secs(60).as_nanos());
            let expires = issued.checked_add(jwt::LIFE.as_nanos())?;
            let input = jwt::signing_input(app, Wall::from_nanos(issued), Wall::from_nanos(expires))?;
            let signature = skein_tls::sign_rs256(key.expose(), &input)?;
            let token = jwt::token(&input, &signature)?;
            self.token = Some(token);
            self.expires = Wall::from_nanos(expires);
        }
        self.token.as_ref()
    }

    /// Drops the held token: a connect begins anew.
    pub(crate) fn forget(&mut self) {
        self.token = None;
        self.expires = Wall::EPOCH;
    }

    fn fresh(&self, wall: Wall, deadline: Duration) -> bool {
        self.token.is_some() && self.expires.as_nanos().saturating_sub(wall.as_nanos()) >= deadline.as_nanos()
    }
}

#[cfg(test)]
mod tests;
