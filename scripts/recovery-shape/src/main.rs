//! Bounded AST checks for the forge client recovery tests and fixtures.
#![expect(
    clippy::disallowed_types,
    clippy::disallowed_methods,
    clippy::disallowed_macros,
    reason = "this host-side AST checker uses standard collections and diagnostics; step-code bans do not apply"
)]
use syn::{
    Expr, ItemFn,
    visit::{self, Visit},
};

#[derive(Default)]
struct StatementShape {
    remake: bool,
    assertion: bool,
}
impl<'ast> Visit<'ast> for StatementShape {
    fn visit_path(&mut self, path: &'ast syn::Path) {
        let mut segments = path.segments.iter().rev();
        if segments.next().is_some_and(|s| s.ident == "Remake") && segments.next().is_some_and(|s| s.ident == "Event") {
            self.remake = true;
        }
        visit::visit_path(self, path);
    }
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        if assertion(&m.path) {
            self.assertion = true;
        }
        visit::visit_macro(self, m);
    }
}

#[derive(Default)]
struct Shape {
    errors: Vec<&'static str>,
    calls: Vec<String>,
    function: String,
}
impl Shape {
    fn error(&mut self, rule: &'static str) {
        self.errors.push(rule);
    }
}
impl<'ast> Visit<'ast> for Shape {
    fn visit_item_fn(&mut self, f: &'ast ItemFn) {
        self.function = f.sig.ident.to_string();
        self.calls.clear();
        // Only the known Remake prelude is assertion-free; its outcome assertions follow Remake.
        if self.function == "remake_releases_a_held_entry" {
            for stmt in &f.block.stmts {
                let mut statement = StatementShape::default();
                statement.visit_stmt(stmt);
                if statement.remake {
                    break;
                }
                if statement.assertion {
                    self.error("The setup the assertion reads");
                }
            }
        }
        visit::visit_block(self, &f.block);
        let wanted = ["uncertain_comment", "one", "answer", "fire", "one", "answer"];
        let mut matched = 0;
        for call in &self.calls {
            if matched < wanted.len() && call == wanted[matched] {
                matched += 1;
            }
        }
        if matched == wanted.len() && self.function != "held_comment" {
            self.error("A setup written once");
        }
    }
    fn visit_expr_call(&mut self, e: &'ast syn::ExprCall) {
        if let Expr::Path(path) = e.func.as_ref() {
            let name = path.path.segments.last().unwrap().ident.to_string();
            self.calls.push(name.clone());
            if ["at", "with_capacity"].contains(&name.as_str()) && e.args.iter().any(integer) {
                self.error("Named fixtures and sizes");
            }
        }
        visit::visit_expr_call(self, e);
    }
    fn visit_expr_method_call(&mut self, e: &'ast syn::ExprMethodCall) {
        self.calls.push(e.method.to_string());
        if e.method == "fire" && e.args.iter().any(integer) {
            self.error("Named fixtures and sizes");
        }
        visit::visit_expr_method_call(self, e);
    }
    fn visit_expr_assign(&mut self, e: &'ast syn::ExprAssign) {
        if let Expr::Field(field) = e.left.as_ref()
            && let syn::Member::Named(member) = &field.member
            && member == "labels"
            && let Expr::Call(call) = e.right.as_ref()
            && call.args.iter().any(|arg| matches!(arg, Expr::Array(a) if a.elems.len() >= 3))
        {
            self.error("Named fixtures and sizes");
        }
        visit::visit_expr_assign(self, e);
    }
    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        if self.function == "held_comment" && assertion(&m.path) {
            self.error("The setup the assertion reads");
        }
        visit::visit_macro(self, m);
    }
}
fn assertion(path: &syn::Path) -> bool {
    path.segments.last().is_some_and(|s| {
        ["assert", "assert_eq", "assert_ne", "debug_assert", "debug_assert_eq", "debug_assert_ne"]
            .iter()
            .any(|name| s.ident == *name)
    })
}
fn integer(e: &Expr) -> bool {
    matches!(e, Expr::Lit(l) if matches!(l.lit, syn::Lit::Int(_)))
}
fn check(source: &str) -> Vec<&'static str> {
    let file = syn::parse_file(source).expect("valid Rust source required");
    let mut shape = Shape::default();
    shape.visit_file(&file);
    shape.errors
}
const GOOD: [&str; 6] = [
    "fn t(){ h.fire(DEADLINE); let x=at(DEADLINE); let _=r#\"h.fire(111);\"#; /* h.fire(111); */ }",
    "fn t(){ let mut renamed=fixture::uncertain_comment(); renamed.other(); }",
    "fn held_comment(){ let mut renamed=fixture::uncertain_comment(); renamed.one(); renamed.answer(x,y); renamed.fire(DEADLINE); renamed.one(); renamed.answer(x,y); }",
    "fn t(){ value.labels=fixture::repeated(empty(), EXCESS_ROWS); }",
    "fn unrelated(){ let rows=[a,b,c,d,e]; value.other=Box::new([a,b,c,d,e]); assert!(ok); }",
    "fn remake_releases_a_held_entry(){ let(h,s)=fixture::held_comment(); h.step(Event::Remake{entry:s}); assert!(ok); }",
];
const BAD: [(&str, &str); 7] = [
    ("fn t(){ renamed.fire(111); }", "Named fixtures and sizes"),
    ("fn t(){ let _=at(111); }", "Named fixtures and sizes"),
    ("fn t(){ let _=List::with_capacity(5); }", "Named fixtures and sizes"),
    ("fn t(){ value.labels=Box::new([a,b,c,d,e]); }", "Named fixtures and sizes"),
    (
        "fn remake_releases_a_held_entry(){ let mut renamed=fixture::uncertain_comment(); let q=renamed.one(); renamed.answer(q,x); /* space */ renamed.fire(DEADLINE); let q=renamed.one(); renamed.answer(q,x); }",
        "A setup written once",
    ),
    ("fn held_comment(){ assert_eq!(x,y); }", "The setup the assertion reads"),
    (
        "fn remake_releases_a_held_entry(){ assert_eq!(x,y); h.step(Event::Remake{entry:s}); }",
        "The setup the assertion reads",
    ),
];
fn controls() {
    for source in GOOD {
        assert!(check(source).is_empty(), "positive: {source}");
    }
    for (source, rule) in BAD {
        assert!(check(source).contains(&rule), "negative: {source}");
    }
    // These strings must neither create an assertion nor hide one before Remake.
    assert!(check(r#"fn remake_releases_a_held_entry(){ let decoy="assert ! assert_eq ! :: Remake"; h.step(Event::Remake{entry:s}); assert!(ok); }"#).is_empty());
    assert!(check(r#"fn remake_releases_a_held_entry(){ let decoy=":: Remake"; assert!(bad); h.step(Event::Remake{entry:s}); }"#).contains(&"The setup the assertion reads"));
    println!("controls: 6 positive, 7 negative and 2 string decoys passed");
}
// Only the two bounded recovery files are passed by scripts/check.sh.
const SOURCE_LIMIT: usize = 256 * 1024; // each file remains below the plan's 800-line ceiling
fn main() {
    let mut bad = false;
    for arg in std::env::args().skip(1) {
        if arg == "--controls" {
            controls();
            continue;
        }
        let source = std::fs::read_to_string(&arg).expect("source file");
        assert!(source.len() <= SOURCE_LIMIT, "recovery source exceeds bounded input size");
        for rule in check(&source) {
            eprintln!("{arg}: testing-strategy.md 10: {rule}");
            bad = true;
        }
    }
    if bad {
        std::process::exit(1);
    }
}
