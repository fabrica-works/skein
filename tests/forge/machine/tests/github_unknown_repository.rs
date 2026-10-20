use skein_fake_forge_github::installations::Reach;
use skein_forge::api::{self, Answer as Value, Error, Op, Read};
use skein_forge_connection::{Access, Request, settings::Kind};
use skein_forge_machine_world::{
    fixtures::{self, FORGE},
    github,
    machine::{Answer, Machine},
};
use skein_lib::{Duration, List, Secret};

const SECOND: Duration = Duration::from_secs(1);
const REPO: api::Repository = api::Repository { forge: FORGE, repository: 2 };
const RESOLVE: &str = "GET /repositories/2";
const MISSING: &str = "GET /repositories/6";
const BRANCH: &str = "GET /repos/acme/widgets/branches/main";
// Two listing pages, two Full/Contents pairs and Viewer, all outside the target call.
const START_REQUESTS: usize = 7;
fn world(calls: &[&'static str]) -> Machine {
    let mut script: Vec<_> = [
        "GET /app/installations?per_page=100&page=1",
        "GET /app/installations?per_page=100&page=2",
        "POST /app/installations/1/access_tokens",
        "POST /app/installations/1/access_tokens",
        "POST /app/installations/2/access_tokens",
        "POST /app/installations/2/access_tokens",
        "POST /graphql",
    ]
    .into_iter()
    .map(Answer::served)
    .collect();
    script.extend(calls.iter().map(|request| Answer::served(request)));
    let mut settings = fixtures::settings();
    let forge = settings.forges.get_mut(0).expect("forge");
    forge.kind = Kind::GitHub;
    forge.access = Some(Access {
        credential: skein_forge::credential::Credential::App { id: 7, key: Secret::new(Box::from(github::app_key())) },
        user: 9,
        webhook_secret: None,
    });
    let mut w = Machine::github(7, fixtures::limits(), settings, script);
    let mut reach = List::with_capacity(1);
    reach.push(4).expect("fixture reach");
    assert!(w.github_installations_mut().set_reach(2, Reach::Listed(reach)));
    w.action(Request::Installations { forge: FORGE, accepted: Box::from([1, 2]) });
    w.run_for(SECOND);
    w
}
fn value(w: &Machine, cost: u32, result: &Result<Value, Error>) {
    let answer = w.answered(1).expect("one terminal");
    assert_eq!((answer.cost, &answer.result), (cost, result));
}
#[test]
fn repository_is_found_on_the_second_installation() {
    // Full/Viewer leave 2 recent. Its Listed [4] cannot answer Repository/2;
    // installation 1's Listed [2,4] can, and owns learned account 22.
    let mut w = world(&[RESOLVE, RESOLVE, BRANCH]);
    w.call(1, REPO, Op::Read(Read::Branch { branch: Box::from(b"main".as_slice()) }));
    w.run_for(SECOND);
    let mut commit = [0; 32];
    commit[..8].copy_from_slice(&1_u64.to_be_bytes());
    value(&w, 3, &Ok(Value::Commit(commit)));
    assert_eq!(w.requests().len(), START_REQUESTS + 3);
    assert!(w.script_done());
    assert!(w.close());
}
#[test]
fn repository_missing_from_every_installation_costs_each_probe() {
    let mut w = world(&[MISSING, MISSING]);
    w.call(
        1,
        api::Repository { repository: 6, ..REPO },
        Op::Read(Read::Branch { branch: Box::from(b"main".as_slice()) }),
    );
    w.run_for(SECOND);
    value(&w, 2, &Err(Error::Missing));
    assert_eq!(w.requests().len(), START_REQUESTS + 2);
    assert!(w.script_done());
    assert!(w.close());
}
#[test]
fn second_installation_settings_needs_no_extra_read() {
    let mut w = world(&[RESOLVE, RESOLVE]);
    w.call(1, REPO, Op::Read(Read::Settings));
    w.run_for(SECOND);
    value(
        &w,
        2,
        &Ok(Value::Settings(api::Settings {
            default_branch: Box::from(b"main".as_slice()),
            merge: true,
            squash: true,
            rebase: false,
        })),
    );
    assert_eq!(w.requests().len(), START_REQUESTS + 2);
    assert!(w.script_done());
    assert!(w.close());
}
