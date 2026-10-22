use super::Fold;

#[test]
fn only_a_moved_path_or_query_folds_disclosures() {
    let mut fold = Fold::default();
    // The platform reads pathname and search only; fragments are no tracker input.
    for (address, moved) in [
        (b"/first".as_slice(), false),
        (b"/first", false),
        (b"/next", true),
        (b"/next?filter=open", true),
        (b"/next?filter=open", false),
        (b"/first", true),
    ] {
        assert_eq!(fold.moved(address), moved, "{address:?}");
    }
}
