use core::hash::Hasher;

use super::sha256::hex;
use crate::{StateHasher, state_digest};

#[test]
fn a_u16_digests_as_its_two_little_endian_bytes() {
    // printf '\002\001' | sha256sum
    let printed = hex("25dfd29c09617dcc9852281c030e5b3037a338a4712a42a21c907f259c6412a0");
    let mut hasher = StateHasher::new();
    hasher.write_u16(0x0102);
    assert_eq!(hasher.digest(), printed, "low byte first, at the integer's own width");
    assert_eq!(state_digest(&0x0102_u16), printed, "a value hashed whole writes the same");
}

#[test]
fn a_usize_digests_as_the_eight_bytes_of_its_u64() {
    // printf '\001\000\000\000\000\000\000\000' | sha256sum
    let printed = hex("7c9fa136d4413fa6173637e883b6998d32e1d675f88cddff9dcbcf331820f4b8");
    assert_eq!(state_digest(&1_usize), printed);
    assert_eq!(state_digest(&1_u64), printed);
}

#[test]
fn a_negative_isize_digests_as_the_eight_bytes_of_its_i64() {
    // printf '\377\377\377\377\377\377\377\377' | sha256sum
    let printed = hex("12a3ae445661ce5dee78d0650d33362dec29c4f82af05e7e57fb595bbbacf0ca");
    assert_eq!(state_digest(&-1_isize), printed);
    assert_eq!(state_digest(&-1_i64), printed);
}

#[test]
fn bytes_written_in_pieces_digest_as_written_once() {
    // printf 'abc' | sha256sum
    let printed = hex("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    let mut pieces = StateHasher::new();
    pieces.write(b"a");
    pieces.write(b"bc");
    let mut whole = StateHasher::new();
    whole.write(b"abc");
    assert_eq!(pieces.digest(), printed);
    assert_eq!(whole.digest(), printed);
}

#[test]
fn finish_gives_the_digest_s_first_eight_bytes_without_ending_it() {
    let mut hasher = StateHasher::new();
    hasher.write_u16(0x0102);
    assert_eq!(hasher.finish(), 0x25df_d29c_0961_7dcc);
    assert_eq!(hasher.finish(), 0x25df_d29c_0961_7dcc);
    hasher.write_u8(3);
    // printf '\002\001\003' | sha256sum
    assert_eq!(hasher.digest(), hex("3ddef33d0729aa4cfb41361985817c9c0e4dca1a67f794530a52716cfcebd60d"));
}

#[test]
fn a_tuple_hashed_whole_digests_its_fields_in_order() {
    // printf '\001\002\000\000\000' | sha256sum
    assert_eq!(state_digest(&(1_u8, 2_u32)), hex("b0b8ad02f2f05ebcc80d37e5324dbf0d88cfd6cb808676f6d7dc5d0b6d3286fa"));
}

use crate::stream::Read;
use crate::{
    CryptoRng, Deadlines, Duration, Intake, Journal, JournalLimits, JournalRoom, List, Map, Queue, Released,
    RequestLimits, RequestTable, Secret, Set, Slab, Stack, Time,
};

#[test]
fn a_queue_digests_its_count_then_its_items() {
    let mut queue = Queue::with_capacity(2);
    queue.push(0x0102_u16);
    // printf '\001\000\000\000\000\000\000\000\002\001' | sha256sum
    assert_eq!(state_digest(&queue), hex("3e6e0bd48ac778ec42f68dd1fffe2291a0df8d48c0e6e10e143747b8df12d725"));
}

#[derive(Clone, Copy, Debug)]
enum Container {
    Map,
    Set,
    Deadlines,
    Queue,
    List,
    Stack,
}
const ORDERED: [Container; 3] = [Container::Map, Container::Set, Container::Deadlines];
const CONTAINERS: [Container; 6] =
    [Container::Map, Container::Set, Container::Deadlines, Container::Queue, Container::List, Container::Stack];

fn container_digest(kind: Container, capacity: u32, items: &[u16]) -> [u8; 32] {
    match kind {
        Container::Map => {
            let mut map = Map::with_capacity(capacity);
            for item in items {
                assert_eq!(map.insert(*item, *item), Ok(None));
            }
            state_digest(&map)
        }
        Container::Set => {
            let mut set = Set::with_capacity(capacity);
            for item in items {
                assert_eq!(set.insert(*item), Ok(true));
            }
            state_digest(&set)
        }
        Container::Deadlines => {
            let mut deadlines = Deadlines::with_capacity(capacity);
            for item in items {
                assert_eq!(deadlines.arm(*item, Time::from_nanos(u64::from(*item))), Ok(()));
            }
            state_digest(&deadlines)
        }
        Container::Queue => {
            let mut queue = Queue::with_capacity(capacity);
            for item in items {
                queue.push(*item);
            }
            state_digest(&queue)
        }
        Container::List => {
            let mut list = List::with_capacity(capacity);
            for item in items {
                assert_eq!(list.push(*item), Ok(()));
            }
            state_digest(&list)
        }
        Container::Stack => {
            let mut stack = Stack::with_capacity(capacity);
            for item in items {
                assert_eq!(stack.push(*item), Ok(()));
            }
            state_digest(&stack)
        }
    }
}

#[test]
fn ordered_containers_filled_in_two_orders_digest_alike() {
    for kind in ORDERED {
        assert_eq!(container_digest(kind, 2, &[1, 2]), container_digest(kind, 2, &[2, 1]), "{kind:?}");
    }
}

#[test]
fn a_queue_that_wrapped_digests_as_one_filled_once() {
    let mut wrapped = Queue::with_capacity(2);
    wrapped.push(9_u16);
    wrapped.push(1);
    assert_eq!(wrapped.pop(), Some(9));
    wrapped.push(2);
    assert_eq!(state_digest(&wrapped), container_digest(Container::Queue, 2, &[1, 2]));
}

#[test]
fn slabs_whose_slots_were_reused_in_one_order_digest_alike() {
    let mut first = Slab::with_capacity(2);
    let x = first.insert('x').expect("room");
    first.insert('z').expect("room");
    first.retire(x);
    first.reclaim();
    first.insert('y').expect("reused room");
    let mut second = Slab::with_capacity(2);
    let x = second.insert('x').expect("room");
    second.retire(x);
    second.reclaim();
    second.insert('y').expect("reused room");
    second.insert('z').expect("room");
    assert_eq!(state_digest(&first), state_digest(&second));
}

#[test]
fn a_change_to_one_entry_changes_a_map_s_digest() {
    let mut map = Map::with_capacity(2);
    assert_eq!(map.insert(1_u16, 2_u16), Ok(None));
    let before = state_digest(&map);
    assert_eq!(map.insert(1, 3), Ok(Some(2)));
    assert_ne!(state_digest(&map), before);
}

#[test]
fn containers_of_any_capacity_holding_the_same_items_digest_alike() {
    for kind in CONTAINERS {
        assert_eq!(container_digest(kind, 2, &[1, 2]), container_digest(kind, 8, &[1, 2]), "{kind:?}");
    }
}

#[test]
fn an_intake_digests_the_bytes_it_holds_alone() {
    let mut first = Intake::with_capacity(4);
    assert_eq!(first.append(b"zab"), Ok(()));
    assert_eq!(first.meet(Read::Fill(1)).as_deref(), Some(b"z".as_slice()));
    assert_eq!(first.append(b"c"), Ok(()));
    let mut second = Intake::with_capacity(16);
    assert_eq!(second.append(b"abc"), Ok(()));
    assert_eq!(second.meet(Read::Line { max: 16 }), None);
    assert_eq!(state_digest(&first), state_digest(&second));
    let mut other = Intake::with_capacity(16);
    assert_eq!(other.append(b"abd"), Ok(()));
    assert_ne!(state_digest(&first), state_digest(&other));
}

#[test]
fn journals_at_one_commit_by_two_histories_digest_alike() {
    let small = JournalLimits { commits: 2, writes: 2, held: 2, now: 2, release: 2 };
    let large = JournalLimits { commits: 4, writes: 4, held: 4, now: 4, release: 4 };
    let mut first = Journal::<u8, u8>::new(&small);
    let mut decision = first.decision(&JournalRoom { writes: 1, held: 1 }).expect("room");
    assert_eq!(decision.write(1), Ok(()));
    assert_eq!(decision.hold(2), Ok(()));
    first.accept(decision);
    assert_eq!(first.commit().expect("commit").number, 1);
    first.committed(1);
    let mut out = Queue::with_capacity(2);
    assert_eq!(first.release(&mut out), Released::Some);
    assert_eq!(out.pop(), Some(2));
    assert_eq!(state_digest(&first), state_digest(&Journal::<u8, u8>::from_durable(&large, 1)));
    assert_ne!(state_digest(&first), state_digest(&Journal::<u8, u8>::from_durable(&large, 2)));
}

#[test]
fn crypto_rngs_with_other_keys_digest_alike_after_the_same_draws() {
    let mut first = CryptoRng::new(&Secret::new(Box::new([1; 32])), 7).expect("32 bytes");
    let mut second = CryptoRng::new(&Secret::new(Box::new([2; 32])), 7).expect("32 bytes");
    first.fill(&mut [0; 70]);
    second.fill(&mut [0; 70]);
    assert_eq!(state_digest(&first), state_digest(&second));
    second.fill(&mut [0; 1]);
    assert_ne!(state_digest(&first), state_digest(&second));
}

#[test]
fn a_request_table_s_digest_changes_with_each_request_asked() {
    let limits = RequestLimits {
        requests: 2,
        bytes: 2,
        out: 2,
        first: Duration::from_secs(1),
        most: Duration::from_secs(8),
        margin: Duration::from_secs(1),
    };
    let mut table = RequestTable::<u8, u8>::new(&limits, 7);
    let mut other = RequestTable::<u8, u8>::new(&RequestLimits { bytes: 8, out: 4, ..limits }, 7);
    assert_eq!(state_digest(&table), state_digest(&other));
    let empty = state_digest(&table);
    assert_eq!(table.ask(1, 1, 1, 1), other.ask(1, 1, 1, 1));
    let one = state_digest(&table);
    assert_ne!(empty, one);
    assert_eq!(one, state_digest(&other));
    assert_eq!(table.ask(2, 1, 1, 2), other.ask(2, 1, 1, 2));
    assert_ne!(one, state_digest(&table));
    assert_eq!(state_digest(&table), state_digest(&other));
}
