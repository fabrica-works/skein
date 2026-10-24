use alloc::boxed::Box;
use skein_io::kernel::{Done, Op};
use skein_lib::Queue;

#[path = "fixtures.rs"]
pub(super) mod fixtures;
use fixtures::{CLOSE, END, HostedInput, INPUT, INPUT_BYTES, ONE_RECORD, READ, WRITE, hosted_input, submit};

#[test]
fn a_hosted_childs_input_closed_with_bytes_unread_reads_them_then_ends() {
    let HostedInput { mut sim, parent, child, writer, reader, .. } = hosted_input();
    let mut completes = Queue::with_capacity(ONE_RECORD);
    submit(&mut sim, parent, WRITE, Op::PipeWrite { fd: writer, bytes: Box::from(&INPUT[..]), from: 0 });
    sim.reap(parent, &mut completes);
    completes.pop().expect("input written");
    submit(&mut sim, parent, CLOSE, Op::Close { fd: writer });
    sim.reap(parent, &mut completes);
    completes.pop().expect("parent input closed");

    submit(&mut sim, child, READ, Op::PipeRead { fd: reader, buf: Box::new([0; INPUT_BYTES]) });
    sim.reap(child, &mut completes);
    let read = completes.pop().expect("buffered input");
    assert_eq!(read.result, Ok(Done::Count(3)));
    let Op::PipeRead { buf, .. } = read.kind else { unreachable!("pipe read") };
    assert_eq!(&*buf, b"abc");

    submit(&mut sim, child, END, Op::PipeRead { fd: reader, buf: Box::new([0; INPUT_BYTES]) });
    sim.reap(child, &mut completes);
    assert_eq!(completes.pop().expect("input end").result, Ok(Done::Count(0)));
}
