//! Values retained by the owner (forge-setup.md, 4.2).
/// Values kept by the host, never paths in the kit.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Item {
    AdminToken,
    ApiToken,
    GitToken,
    User,
    WebhookSecret,
}
