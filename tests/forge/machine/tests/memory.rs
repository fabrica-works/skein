use skein_forge_connection::limits::{self, Forges};
use skein_forge_machine_world::{fixtures, memory};

#[global_allocator]
static HEAP: skein_heap::Counting = skein_heap::Counting;

#[test]
fn full_forgejo_holders_fit_the_declared_worst_case() {
    check(Forges { forgejo: 2, github: 0 }, [2, 2, 2, 2, 8, 4, 0, 0, 2, 8, 8, 0, 0, 65408, 4]);
}
#[test]
fn full_github_holders_fit_the_declared_worst_case() {
    check(Forges { forgejo: 0, github: 2 }, [2, 2, 4, 2, 8, 4, 6, 12, 2, 8, 1, 8, 1, 65408, 0]);
}
#[test]
fn full_mixed_holders_fit_the_declared_worst_case() {
    check(Forges { forgejo: 1, github: 1 }, [2, 2, 3, 4, 16, 4, 3, 6, 2, 8, 1, 8, 1, 65408, 4]);
}
fn check(forges: Forges, saturation: [u32; 15]) {
    let full = memory::full_heap(forges);
    assert_eq!(
        full.bound,
        limits::worst_case(&fixtures::limits(), &fixtures::shared(), forges, full.owned_bytes).expect("bound")
    );
    assert_eq!(full.saturation(), saturation);
    assert_eq!((full.op_bytes, full.body_bytes, full.rows, full.inner), (512, 1042, 8, 8));
    assert_eq!(full.answer_bytes, 65_536);
    if forges.github > 0 {
        assert_eq!(full.key_bytes, 1192);
        assert!(full.jwt_bytes > 0);
    }
    assert!(full.owned > 0 && full.owned <= full.bound);
    println!(
        "{forges:?}: owned={}, bound={}, margin={}, saturation={:?}, key_bytes={}, jwt_bytes={}",
        full.owned,
        full.bound,
        full.bound - full.owned,
        full.saturation(),
        full.key_bytes,
        full.jwt_bytes
    );
}
