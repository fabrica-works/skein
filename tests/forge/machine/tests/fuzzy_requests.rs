use skein_forge::api::{Answer as ResultAnswer, Error};
use skein_forge_machine_world::{
    fixtures,
    machine::{Connect, Cut, Machine, Response},
};
use skein_lib::Duration;
const DEADLINE: Duration = Duration::from_secs(10);
// Two start reads precede the first Locate, whose generated request is charged once.
const CALL: usize = 2;
#[test]
fn plaintext_request_cuts_end_once() {
    let mut control = Machine::new(7, fixtures::locate_script());
    control.call(1, fixtures::REPO, fixtures::locate());
    control.run_for(DEADLINE);
    assert_eq!(
        (control.answered(1).expect("control").cost, &control.answered(1).expect("control").result),
        (1, &Ok(ResultAnswer::Located { repository: 1 }))
    );
    let requests = control.requests().iter().map(Vec::len).collect::<Vec<_>>();
    assert!(control.close());
    for (request, length) in requests.into_iter().enumerate() {
        let response = match fixtures::locate_script().remove(request).response {
            Response::Bytes(bytes) => bytes,
            Response::Case(_) | Response::Reset | Response::Cut { .. } | Response::Silent => {
                unreachable!("generated response")
            }
        };
        let head = response.windows(4).position(|bytes| bytes == b"\r\n\r\n").expect("head") + 4;
        for (is_response, length) in [(false, length), (true, head)] {
            for reset in [false, true] {
                for at in 0..=length {
                    let cut = Cut { request, response: is_response, at, reset };
                    let result = drive_cut(cut);
                    let written = u32::from(request == CALL && (is_response || at > 0));
                    let error = if written == 0 { Error::Unavailable } else { Error::Timeout };
                    assert_eq!(result, (written, written, error, 1, 1), "{cut:?}");
                }
            }
        }
    }
}
fn drive_cut(cut: Cut) -> (u32, u32, Error, usize, u32) {
    let mut world = Machine::new(7, fixtures::locate_script());
    let mut connects = [Connect::Refused; 16];
    connects[0] = Connect::Made;
    world.connects(&connects);
    world.cut(cut);
    world.call(1, fixtures::REPO, fixtures::locate());
    world.run_for(DEADLINE);
    let answer = world.answered(1).expect("cut ends the call");
    let result = (answer.cost, answer.result.clone().expect_err("cut failure"), world.deliveries().len());
    let written = u32::from(world.written_bytes().get(CALL).copied().unwrap_or(0) > 0);
    assert!(world.close());
    (written, result.0, result.1, result.2, world.closed_events())
}
