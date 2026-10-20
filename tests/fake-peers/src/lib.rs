use skein_io::kernel::{Done, Op, Submit};
use skein_lib::{Queue, Token};
use skein_sim::{Pid, Sim};

/// Submit one scripted client operation and drive it to completion.
pub fn call(sim: &mut Sim, pid: Pid, token: u64, kind: Op) -> Done {
    let mut submissions = Queue::with_capacity(1);
    let mut completions = Queue::with_capacity(1);
    submissions.push(Submit { op: Token::new(token), kind });
    sim.submit(pid, &mut submissions);
    for _ in 0..100 {
        sim.reap(pid, &mut completions);
        if let Some(complete) = completions.pop() {
            return complete.result.expect("scripted client's operation succeeds");
        }
        assert!(sim.advance(), "the client operation must complete");
    }
    panic!("the client operation did not settle");
}

/// The named observation cap, rather than another failure, ended this exchange.
pub fn assert_panic<T: std::fmt::Debug>(result: std::thread::Result<T>, expected: &str) {
    let panic = result.expect_err("recording at cap zero panics");
    let message = panic
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| panic.downcast_ref::<&str>().copied())
        .expect("assertion message");
    assert!(message.contains(expected), "{message}");
}

/// A non-recording host's envelope has no record slots or payload allowance.
pub fn assert_record_bound<T>(before: u64, after: u64, count: u32, bytes: u32) {
    assert_eq!(
        before - after,
        u64::from(count) * u64::try_from(std::mem::size_of::<T>()).expect("record size fits")
            + u64::from(bytes)
    );
}
