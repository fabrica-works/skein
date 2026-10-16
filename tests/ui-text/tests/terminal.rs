use skein_io::kernel::Fd;
use skein_lib::{Duration, Time};
use skein_sim::{Config, Pid, Sim};
use skein_ui::Act;
use skein_ui::Tree;
use skein_ui_text::{Command, Settings, Terminal};
use skein_ui_text_tests as fixtures;

struct Pipes {
    sim: Sim,
    pid: Pid,
    input: Fd,
    output: Fd,
    terminal: Terminal,
    tree: Tree,
}
fn settings(longest: u32) -> Settings {
    Settings { name: b"test".as_slice().into(), longest, interval: Duration::from_secs(1), status: None, lines: 2 }
}
fn start(longest: u32, buffer: u32) -> Pipes {
    let mut config = Config::calm();
    config.buffer = buffer;
    let mut sim = Sim::new(3, config);
    let pid = sim.spawn_process();
    let input = sim.open_inherited_read(pid);
    let output = sim.open_inherited_write(pid);
    let mut terminal = Terminal::new(settings(longest), &fixtures::LIMITS).expect("the settings fit the limits");
    terminal.adopt(input, output).expect("a terminal adopts its pipes once");
    let tree = fixtures::page(|b| fixtures::button(b, b"Accept", true));
    let mut p = Pipes { sim, pid, input, output, terminal, tree };
    p.drive(Time::ZERO, false);
    p
}
impl Pipes {
    fn drive(&mut self, now: Time, changed: bool) {
        self.sim.advance_to(now);
        for tick in 0_u32..20 {
            self.sim.reap(self.pid, self.terminal.completions());
            self.terminal.iterate(now, &self.tree, changed && tick == 0);
            self.sim.submit(self.pid, self.terminal.submissions());
        }
    }
    fn feed(&mut self, bytes: &[u8]) {
        assert_eq!(self.sim.peer_feed(self.pid, self.input, bytes), bytes.len());
        self.drive(Time::ZERO, false);
    }
    fn drain(&mut self) -> Box<[u8]> {
        self.sim.peer_drain(self.pid, self.output, 65536).into_boxed_slice()
    }
    fn act(&mut self) -> Command {
        self.terminal.commands().pop().expect("one command")
    }
    fn expected(&self) -> Command {
        let mut node = skein_ui::NodeId(0);
        for n in self.tree.nodes() {
            if n.role == skein_ui::Role::Button {
                node = n.id;
            }
        }
        Command::Act { node, first: Act::Press, submit: false }
    }
}
#[test]
fn a_line_split_across_reads_is_one_command() {
    let mut p = start(32, 4096);
    p.feed(b"1\r");
    assert_eq!(p.terminal.commands().len(), 0);
    p.feed(b"\n");
    assert_eq!(p.act(), p.expected());
    assert_eq!(p.terminal.commands().len(), 0);
}
#[test]
fn two_lines_in_one_read_are_two_commands() {
    let mut p = start(32, 4096);
    p.feed(b"1\n1\n");
    assert_eq!(p.act(), p.expected());
    assert_eq!(p.act(), p.expected());
    assert_eq!(p.terminal.commands().len(), 0);
}
#[test]
fn a_line_past_the_longest_is_refused_through_its_newline() {
    let mut p = start(6, 4096);
    p.drain();
    p.feed(b"1234567");
    assert_eq!(p.terminal.commands().len(), 0);
    assert!(p.drain().is_empty());
    p.feed(b"discarded\n1\n");
    assert_eq!(p.act(), p.expected());
    assert!(p.drain().starts_with(b"test: line too long\n"));
}
#[test]
fn changes_within_the_interval_print_once_when_it_ends() {
    let mut p = start(32, 4096);
    assert_eq!(p.drain().as_ref(), b"== test == \n[1] Accept\n");
    p.drive(Time::from_nanos(100), true);
    p.drive(Time::from_nanos(200), true);
    assert!(p.drain().is_empty());
    assert_eq!(p.terminal.next_deadline(), Some(Time::from_nanos(1_000_000_000)));
    p.drive(Time::from_nanos(1_000_000_000), false);
    assert_eq!(p.drain().as_ref(), b"== test == \n[1] Accept\n");
    p.drive(Time::from_nanos(2_000_000_000), false);
    assert!(p.drain().is_empty());
}
#[test]
fn a_print_waits_for_the_last_to_be_written() {
    let mut p = start(32, 1);
    p.feed(b"p");
    p.feed(b"\n");
    let mut text = skein_lib::List::with_capacity(256);
    for _ in 0_u32..80 {
        for byte in &p.drain() {
            text.push(*byte).unwrap();
        }
        p.drive(Time::ZERO, false);
    }
    assert_eq!(text.as_slice(), b"== test == \n[1] Accept\n== test == \n[1] Accept\n");
}
#[test]
fn the_applications_lines_keep_their_room_while_the_output_is_held() {
    let mut p = start(32, 1);
    assert_eq!(p.terminal.room(), 2);
    p.terminal.say(b"one");
    p.terminal.say(b"two");
    assert_eq!(p.terminal.room(), 0);
    for byte in b"x\ny\n" {
        p.feed(&[*byte]);
    }
    let mut text = skein_lib::List::with_capacity(256);
    for _ in 0_u32..160 {
        for byte in &p.drain() {
            text.push(*byte).unwrap();
        }
        p.drive(Time::ZERO, false);
    }
    assert_eq!(
        text.as_slice(),
        b"== test == \n[1] Accept\ntest: one\ntest: two\ntest: commands: n, n words, p, q\ntest: commands: n, n words, p, q\n"
    );
    assert_eq!(p.terminal.room(), 2);
    p.terminal.close();
    assert_eq!(p.terminal.room(), 0);
}
#[test]
fn untaken_commands_hold_the_input() {
    let mut p = start(32, 1);
    p.feed(b"1");
    p.feed(b"\n");
    for _ in 0..64 {
        p.feed(b"p");
    }
    assert_eq!(p.sim.peer_feed(p.pid, p.input, b"p"), 1);
    assert_eq!(p.sim.peer_feed(p.pid, p.input, b"\n"), 0);
    assert_eq!(p.act(), p.expected());
    p.drive(Time::ZERO, false);
    assert_eq!(p.sim.peer_feed(p.pid, p.input, b"\n"), 1);
}
#[test]
fn the_applications_line_comes_before_the_next_print() {
    let mut p = start(32, 4096);
    p.drain();
    p.terminal.say(b"credential");
    p.feed(b"p\n");
    assert_eq!(p.drain().as_ref(), b"test: credential\n== test == \n[1] Accept\n");
}
#[test]
fn the_inputs_end_closes_without_a_new_print() {
    let mut p = start(32, 4096);
    p.drain();
    p.sim.peer_close(p.pid, p.input);
    p.drive(Time::ZERO, true);
    assert_eq!(p.drain().as_ref(), b"test: closed\n");
    assert!(p.terminal.closed());
    p.sim.assert_no_open_fds(p.pid);
}
#[test]
fn a_name_longer_than_a_nodes_text_is_refused() {
    for (extra, accepted) in [(0, true), (1, false)] {
        let mut config = settings(32);
        config.name = skein_lib::bytes::zeroed(usize::try_from(fixtures::LIMITS.text_per_node).unwrap() + extra);
        assert_eq!(Terminal::new(config, &fixtures::LIMITS).is_some(), accepted);
    }
}

#[test]
fn closing_finishes_the_print_it_began() {
    for quit in [false, true] {
        let mut p = start(32, 1);
        if quit {
            p.feed(b"q");
            p.feed(b"\n");
        } else {
            p.terminal.close();
        }
        let mut text = skein_lib::List::with_capacity(256);
        for _ in 0_u32..80 {
            for byte in &p.drain() {
                text.push(*byte).unwrap();
            }
            p.drive(Time::ZERO, true);
        }
        assert_eq!(text.as_slice(), b"== test == \n[1] Accept\ntest: closed\n", "quit={quit}");
        assert!(p.terminal.closed());
        p.sim.assert_no_open_fds(p.pid);
    }
}
