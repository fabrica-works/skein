//! Only the three sampled 03.3 functions and their extracted terminal matrix.
use syn::{
    Expr, ItemFn, Member, Token,
    punctuated::Punctuated,
    visit::{self, Visit},
};
const BEHAVIOUR: &str = "One behaviour per test";
const SIZES: &str = "Named fixtures and sizes";
const PAGE: &str = "each_listing_uses_its_largest_answer_row";
const EVICTION: &str = "an_idle_connection_is_reassigned_to_another_forge";
const BACKOFF: &str = "start_backoff_doubles_to_its_maximum";
const TERMINAL: &str = "terminal_start_states_ignore_every_start_answer";
#[derive(Default)]
struct Shape {
    function: String,
    errors: Vec<&'static str>,
}
impl Shape {
    fn error(&mut self, rule: &'static str) {
        if !self.errors.contains(&rule) {
            self.errors.push(rule);
        }
    }
    fn size(&mut self, e: &Expr) {
        // Walk expression syntax, including arithmetic; strings/comments cannot supply a size.
        struct Numbers(bool);
        impl<'a> Visit<'a> for Numbers {
            fn visit_lit_int(&mut self, _: &'a syn::LitInt) {
                self.0 = true;
            }
        }
        let mut n = Numbers(false);
        n.visit_expr(e);
        if n.0 {
            self.error(SIZES);
        }
    }
}
impl<'a> Visit<'a> for Shape {
    fn visit_item_fn(&mut self, f: &'a ItemFn) {
        self.function = f.sig.ident.to_string();
        if [PAGE, EVICTION, BACKOFF, TERMINAL].contains(&self.function.as_str()) {
            self.visit_block(&f.block);
        }
    }
    fn visit_path(&mut self, path: &'a syn::Path) {
        if self.function == BACKOFF
            && path.segments.last().is_some_and(|s| ["heard", "Ready", "Stopped"].iter().any(|n| s.ident == *n))
        {
            self.error(BEHAVIOUR);
        }
        visit::visit_path(self, path);
    }
    fn visit_expr_tuple(&mut self, t: &'a syn::ExprTuple) {
        if self.function == PAGE && t.elems.len() == 4 {
            for e in t.elems.iter().skip(1) {
                self.size(e);
            }
            // The subtraction in row - 1 is a byte boundary, not a capacity.
            if let Some(Expr::Binary(b)) = t.elems.first()
                && matches!(b.op, syn::BinOp::Mul(_))
            {
                self.size(&b.left);
                self.size(&b.right);
            }
        }
        visit::visit_expr_tuple(self, t);
    }
    fn visit_field_value(&mut self, f: &'a syn::FieldValue) {
        if self.function == EVICTION
            && let Member::Named(n) = &f.member
            && (n == "connections" || n == "calls")
        {
            self.size(&f.expr);
        }
        visit::visit_field_value(self, f);
    }
    fn visit_expr_method_call(&mut self, e: &'a syn::ExprMethodCall) {
        if self.function == TERMINAL && (e.method == "refuse" || e.method == "run") {
            self.error(BEHAVIOUR);
        }
        if self.function == EVICTION && e.method == "checked_add" {
            for arg in &e.args {
                self.size(arg);
            }
        }
        visit::visit_expr_method_call(self, e);
    }
    fn visit_macro(&mut self, m: &'a syn::Macro) {
        if m.path.is_ident("assert_eq") {
            let args =
                m.parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated).expect("assert_eq expressions");
            if self.function == EVICTION
                && let Some(Expr::MethodCall(lhs)) = args.first()
                && (lhs.method == "len" || lhs.method == "connections_for")
                && let Some(rhs) = args.iter().nth(1)
            {
                self.size(rhs);
            }
            if self.function == EVICTION
                && matches!(args.first(), Some(Expr::Tuple(_)))
                && let Some(rhs) = args.iter().nth(1)
            {
                self.size(rhs);
            }
            for arg in &args {
                self.visit_expr(arg);
            }
        }
        visit::visit_macro(self, m);
    }
}
pub fn check(source: &str) -> Vec<&'static str> {
    let f = syn::parse_file(source).expect("valid bounded Rust source");
    let mut s = Shape::default();
    s.visit_file(&f);
    s.errors
}
pub fn controls() {
    let good = [
        "fn start_backoff_doubles_to_its_maximum(){ h.refuse(FORGE); assert_eq!(h.start(FORGE), Start::Waiting {until: END,next: NEXT}); let _=\"Start::Ready heard\"; }",
        "fn terminal_start_states_ignore_every_start_answer(){ for state in [Start::Ready{cap:CAP}, Start::Stopped(REASON)] { assert_eq!(start::heard(state,answer,user,now,limits),state); } }",
        "fn each_listing_uses_its_largest_answer_row(){ let rows=[(row-1, TWO,TWO,ZERO), (TWO*row,TWO,ONE,ONE)]; }",
        "fn an_idle_connection_is_reassigned_to_another_forge(){ let l=Limits{connections:ONE,calls:ONE}; assert_eq!((h.connections_for(A),h.connections_for(B)),(ZERO,ONE)); assert_eq!(h.answers.len(), TWO); }",
        "fn inherited(){ let l=Limits{connections:1,calls:1}; assert_eq!(h.answers.len(),2); start::heard(); }",
    ];
    for s in good {
        assert!(check(s).is_empty(), "positive/decoy: {s}");
    }
    let bad = [
        ("fn start_backoff_doubles_to_its_maximum(){ /* Start::Waiting */ let x=Start::Ready{cap:CAP}; }", BEHAVIOUR),
        (
            "fn start_backoff_doubles_to_its_maximum(){ let _=\"separate terminal test\"; start::heard(state,answer,user,now,limits); }",
            BEHAVIOUR,
        ),
        ("fn terminal_start_states_ignore_every_start_answer(){ renamed.refuse(FORGE); }", BEHAVIOUR),
        (
            "fn each_listing_uses_its_largest_answer_row(){ let _=\"PAGE_ROWS\"; let rows=[(row, TWO, ONE+1, ONE)]; }",
            SIZES,
        ),
        ("fn each_listing_uses_its_largest_answer_row(){ let rows=[(2*row,TWO,TWO,TWO)]; }", SIZES),
        ("fn an_idle_connection_is_reassigned_to_another_forge(){ let l=Limits{connections:1,calls:ONE}; }", SIZES),
        ("fn an_idle_connection_is_reassigned_to_another_forge(){ assert_eq!(renamed.answers.len(), 1+1); }", SIZES),
        (
            "fn an_idle_connection_is_reassigned_to_another_forge(){ assert_eq!((h.connections_for(A),h.connections_for(B)),(0,1)); }",
            SIZES,
        ),
        (
            "fn an_idle_connection_is_reassigned_to_another_forge(){ assert_eq!(h.connections_opened(),before.checked_add(1).expect(\"ONE\")); }",
            SIZES,
        ),
    ];
    for (s, r) in bad {
        assert_eq!(check(s), vec![r], "negative: {s}");
    }
    println!("connection-shapes controls: 5 positive/decoy, 9 negative passed");
}
