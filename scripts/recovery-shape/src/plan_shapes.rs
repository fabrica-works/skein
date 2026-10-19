//! Only the four sampled plan tests: testing-strategy.md 10, One behaviour per test.
use syn::{
    Expr, Item, ItemFn,
    parse::Parser,
    punctuated::Punctuated,
    visit::{self, Visit},
};
const FUNCTIONS: [&str; 4] = [
    "subscriptions_match_the_literal_contract",
    "deliveries_map_subscribed_events_to_hints",
    "marker_words_at_both_bounds_are_valid",
    "markers_accept_words_at_both_bounds",
];
#[derive(Default)]
struct Contracts {
    subscribed: bool,
    mapping: bool,
    validity: bool,
    round_trip: bool,
    in_assertion: bool,
}
impl<'ast> Visit<'ast> for Contracts {
    fn visit_item_fn(&mut self, _: &'ast ItemFn) {} // Nested helpers have their own contract.
    fn visit_path(&mut self, path: &'ast syn::Path) {
        if self.in_assertion && ends(path, &["delivery", "SUBSCRIBED"]) {
            self.subscribed = true;
        }
        visit::visit_path(self, path);
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if let Expr::Path(path) = call.func.as_ref() {
            self.mapping |= ends(&path.path, &["delivery", "header"]) || ends(&path.path, &["hint"]);
            self.validity |= self.in_assertion && ends(&path.path, &["marker", "valid_word"]);
            self.round_trip |= ["length", "encode", "decode"].iter().any(|name| ends(&path.path, &["marker", name]));
        }
        visit::visit_expr_call(self, call);
    }
    fn visit_macro(&mut self, mac: &'ast syn::Macro) {
        if super::assertion(&mac.path) {
            let arguments = Punctuated::<Expr, syn::Token![,]>::parse_terminated
                .parse2(mac.tokens.clone())
                .expect("assertion expression arguments");
            let previous = self.in_assertion;
            self.in_assertion = true;
            for argument in &arguments {
                self.visit_expr(argument);
            }
            self.in_assertion = previous;
        }
    }
}
fn ends(path: &syn::Path, names: &[&str]) -> bool {
    path.segments.len() >= names.len() && path.segments.iter().rev().zip(names.iter().rev()).all(|(s, n)| s.ident == *n)
}
pub(super) fn check(source: &str) -> Vec<&'static str> {
    let file = syn::parse_file(source).expect("valid Rust source required");
    let mut errors = Vec::new();
    for item in &file.items {
        if let Item::Fn(function) = item
            && FUNCTIONS.iter().any(|name| function.sig.ident == *name)
        {
            let mut contracts = Contracts::default();
            contracts.visit_block(&function.block);
            if (contracts.subscribed && contracts.mapping) || (contracts.validity && contracts.round_trip) {
                errors.push("One behaviour per test");
            }
        }
    }
    errors
}
const BAD: [&str; 6] = [
    "fn deliveries_map_subscribed_events_to_hints(){ assert_eq!(delivery::SUBSCRIBED, LIST); for d in rows { assert_eq!(delivery::header(d), h); assert_eq!(hint(d), expected); } }",
    "fn markers_accept_words_at_both_bounds(){ for (w,n) in MARKERS { assert!(marker::valid_word(w)); assert_eq!(marker::length(w), n); let e=marker::encode(w); assert_eq!(marker::decode(e), key); } }",
    "fn subscriptions_match_the_literal_contract(){ core::assert_eq!(crate::delivery::SUBSCRIBED, LIST); core::assert_eq!(crate::delivery::header(d), h); }",
    "fn marker_words_at_both_bounds_are_valid(){ std::assert!((crate::marker::valid_word(w))); std::assert_eq!((crate::marker::decode(e)), key); }",
    "fn deliveries_map_subscribed_events_to_hints(){ assert_eq!(hint(d), expected); assert_eq!((delivery::SUBSCRIBED), LIST); }",
    "fn markers_accept_words_at_both_bounds(){ let e=marker::encode(w); assert_eq!(marker::decode(e), key); for w in words { assert!(marker::valid_word(w)); } }",
];
const GOOD: [&str; 6] = [
    "fn subscriptions_match_the_literal_contract(){ assert_eq!(delivery::SUBSCRIBED, LIST); } fn deliveries_map_subscribed_events_to_hints(){ assert_eq!(delivery::header(d), h); assert_eq!(hint(d), expected); }",
    "fn marker_words_at_both_bounds_are_valid(){ for w in words { assert!(marker::valid_word(w)); } } fn markers_accept_words_at_both_bounds(){ assert_eq!(marker::length(w), n); let e=marker::encode(w); assert_eq!(marker::decode(e), key); }",
    r#"fn deliveries_map_subscribed_events_to_hints(){ let decoy="assert_eq!(delivery::SUBSCRIBED, LIST)"; assert_eq!(hint(d), expected); }"#,
    "fn markers_accept_words_at_both_bounds(){ /* assert!(marker::valid_word(w)); */ assert_eq!(marker::decode(e), key); }",
    "fn unrelated(){ assert!(marker::valid_word(w)); let e=marker::encode(w); assert_eq!(delivery::SUBSCRIBED, LIST); assert_eq!(hint(d), expected); }",
    "fn marker_words_at_both_bounds_are_valid(){ assert!(marker::valid_word(w)); fn helper(){ let e=marker::encode(w); assert_eq!(marker::decode(e), key); } }",
];
pub(super) fn controls() {
    for source in BAD {
        assert_eq!(check(source), ["One behaviour per test"], "negative: {source}");
    }
    for source in GOOD {
        assert!(check(source).is_empty(), "positive: {source}");
    }
    println!("plan-shapes controls: 6 negative and 6 positive passed (including 2 decoys)");
}
