use skein_forge::api::Error;
use skein_forge_machine_world::{
    fixtures,
    machine::{self, Answer, Machine, Response},
};
use skein_lib::Duration;
#[test]
fn a_response_cut_at_each_byte_ends_its_call_once() {
    sweep_seed(0);
}
#[test]
fn a_response_cut_at_each_byte_ends_its_call_once_seed_1() {
    sweep_seed(1);
}
#[test]
fn a_response_cut_at_each_byte_ends_its_call_once_seed_2() {
    sweep_seed(2);
}
#[test]
fn a_response_cut_at_each_byte_ends_its_call_once_seed_3() {
    sweep_seed(3);
}
fn sweep_seed(seed: u64) {
    let length = machine::case_bytes("branch").len();
    for at in 0..length {
        let mut world = Machine::new(seed, cut_script(at));
        world.call(1, fixtures::REPO, fixtures::branch());
        world.run_for(Duration::from_secs(1));
        let answered = world.answered(1).unwrap_or_else(|| panic!("seed {seed}, cut at {at}: no answer"));
        assert_eq!((answered.cost, &answered.result), (2, &Err(Error::Timeout)), "seed {seed}, cut at {at}");
        assert!(world.close(), "seed {seed}, cut at {at}: close");
    }
}
// Startup and resolution are covered by the recorded focused cases. Keep the
// cut sweep's setup small: only the branch response under fault is recorded.
fn cut_script(at: usize) -> Vec<Answer> {
    vec![
        Answer { request: "GET /api/v1/user", response: Response::Bytes(fixtures::response(200, br#"{"id":2,"login":"writer"}"#)) },
        Answer { request: "GET /api/v1/settings/api", response: Response::Bytes(fixtures::response(200, br#"{"max_response_items":2,"default_paging_num":30}"#)) },
        Answer { request: "GET /api/v1/repositories/1", response: Response::Bytes(fixtures::response(200, br#"{"id":1,"owner":{"id":1,"login":"fixture"},"name":"specimen","object_format_name":"sha1","default_branch":"main","default_merge_style":"merge","allow_merge_commits":true,"allow_squash_merge":true,"allow_rebase":true,"html_url":"http://127.0.0.1:3000/fixture/specimen"}"#)) },
        Answer { request: "GET /api/v1/repos/fixture/specimen/branches/main", response: Response::Cut { case: "branch", at } },
    ]
}
