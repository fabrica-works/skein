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
