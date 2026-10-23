use super::{CALM, ENGINE, FIRST, MAIN, STRANGER};
use crate::{self as domain, Config, Domain, Signature, api::Error};
use skein_lib::{Env, Time, Wall};
const NAME: &[u8] = b"org/first";
fn signature() -> Signature {
    Signature { name: b"kit".as_slice().into(), email: b"kit@invalid".as_slice().into() }
}
fn configured() -> Config {
    let mut config = CALM;
    config.limits.repositories = 1;
    config.limits.files = 1;
    config
}
fn created(config: &Config) -> Domain {
    let mut domain = Domain::new(config, 1);
    domain::create_repository(&mut domain, config, 1, NAME, b"first", ENGINE).expect("repository fits");
    domain
}
fn file(domain: &mut Domain, config: Config, by: u64) -> Result<u64, Error> {
    let env = Env { now: Time::ZERO, wall: Wall::EPOCH, limits: config };
    domain::create_file(domain, &env, NAME, MAIN, b"file", b"bytes", b"Add file", signature(), by)
}
#[test]
fn a_repository_and_readme_fit_the_last_available_slots() {
    let config = configured();
    let mut domain = Domain::new(&config, 1);
    let result = domain::create_repository(&mut domain, &config, 1, NAME, b"first", ENGINE);
    assert_eq!(result, Ok(FIRST));
    assert_eq!(
        domain.object(FIRST).expect("readme").tree.get(b"README.md".as_slice()).expect("readme bytes").as_ref(),
        b"# first\n"
    );
}
#[test]
fn a_full_repository_table_refuses_without_allocating_a_commit() {
    let config = configured();
    let mut domain = created(&config);
    let room = domain.room().commits;
    assert_eq!(domain::create_repository(&mut domain, &config, 2, b"org/second", b"second", ENGINE), Err(Error::Full));
    assert_eq!(domain.room().commits, room);
}
#[test]
fn a_full_tree_refuses_a_file_without_allocating_a_commit() {
    let config = configured();
    let mut domain = created(&config);
    let room = domain.room().commits;
    assert_eq!(file(&mut domain, config, ENGINE), Err(Error::Full));
    assert_eq!(domain.room().commits, room);
    assert_eq!(domain.branch(NAME, MAIN), Some(FIRST));
}
#[test]
fn a_file_write_requires_permission_before_moving_main() {
    let config = CALM;
    let mut domain = created(&config);
    let room = domain.room().commits;
    assert_eq!(file(&mut domain, config, STRANGER), Err(Error::Forbidden));
    assert_eq!(domain.room().commits, room);
    assert_eq!(domain.branch(NAME, MAIN), Some(FIRST));
}
