//! Only the size tables and REMARKS slice bounds in two top-level listing tests.
use syn::{
    Expr, Item, ItemFn, ItemMod,
    visit::{self, Visit},
};

const RULE: &str = "Named fixtures and sizes";
const LISTINGS: &str = "page_size_is_passed_to_each_listing";
const REMARKS: &str = "remarks_are_sliced_without_wire_paging";

#[derive(Default)]
struct Numbers(bool);
impl<'ast> Visit<'ast> for Numbers {
    fn visit_lit_int(&mut self, _: &'ast syn::LitInt) {
        self.0 = true;
    }
}

#[derive(Default)]
struct Shape {
    errors: Vec<&'static str>,
}
impl Shape {
    fn size(&mut self, expression: &Expr) {
        let mut numbers = Numbers::default();
        numbers.visit_expr(expression);
        if numbers.0 {
            self.errors.push(RULE);
        }
    }
}
impl<'ast> Visit<'ast> for Shape {
    // Nested functions and modules have their own scope, outside the sampled tests.
    fn visit_item_fn(&mut self, _: &'ast ItemFn) {}
    fn visit_item_mod(&mut self, _: &'ast ItemMod) {}

    fn visit_expr_for_loop(&mut self, expression: &'ast syn::ExprForLoop) {
        if let syn::Pat::Tuple(binding) = expression.pat.as_ref()
            && let Some(syn::Pat::Ident(first)) = binding.elems.first()
            && first.ident == "size"
            && let Expr::Array(table) = expression.expr.as_ref()
        {
            for row in &table.elems {
                if let Expr::Tuple(cells) = row
                    && let Some(size) = cells.elems.first()
                {
                    self.size(size);
                }
            }
        }
        visit::visit_expr_for_loop(self, expression);
    }

    fn visit_expr_index(&mut self, expression: &'ast syn::ExprIndex) {
        if let Expr::Path(array) = expression.expr.as_ref()
            && array.path.is_ident("REMARKS")
            && let Expr::Range(range) = expression.index.as_ref()
            && let Some(upper) = &range.end
        {
            self.size(upper);
        }
        visit::visit_expr_index(self, expression);
    }
}

pub fn check(source: &str) -> Vec<&'static str> {
    let file = syn::parse_file(source).expect("valid bounded Rust source");
    let mut shape = Shape::default();
    for item in &file.items {
        if let Item::Fn(function) = item
            && (function.sig.ident == LISTINGS || function.sig.ident == REMARKS)
        {
            shape.visit_block(&function.block);
        }
    }
    shape.errors
}

pub fn controls() {
    let good = [
        "fn page_size_is_passed_to_each_listing(){ for (size, wire) in [(PAGE_SIZE,b\"2\"),(ROWS,b\"8\"),(ONE_ROW_PAGE,b\"1\")] {} }",
        "fn remarks_are_sliced_without_wire_paging(){ for (size, expected, more) in [(PAGE_SIZE,&REMARKS[..page_rows],true),(ROWS,&REMARKS[..],false),(ONE_ROW_PAGE,&REMARKS[..one_row],true)] {} }",
        "fn page_size_is_passed_to_each_listing(){ /* [(1,b\"1\")] REMARKS[..2] */ let text=r#\"REMARKS[..1] size=2\"#; let id=7; let rows=[(1,b\"2\")]; for (id,wire) in [(1,b\"1\")] {} let _=&OTHER[..2]; }",
        "fn unrelated(){ for (size,wire) in [(1,b\"1\")] {} let _=&REMARKS[..2]; } mod other { fn remarks_are_sliced_without_wire_paging(){ let _=&REMARKS[..2]; } } fn page_size_is_passed_to_each_listing(){ fn nested(){ for (size,wire) in [(1,b\"1\")] {} let _=&REMARKS[..2]; } }",
    ];
    for source in good {
        assert!(check(source).is_empty(), "positive: {source}");
    }
    let bad = [
        "fn page_size_is_passed_to_each_listing(){ for (size,wire) in [(1,b\"1\")] {} }",
        "fn page_size_is_passed_to_each_listing(){ for (size,wire) in [((2),b\"2\")] {} }",
        "fn remarks_are_sliced_without_wire_paging(){ for (size,wire) in [(8u32,b\"8\")] {} }",
        "fn page_size_is_passed_to_each_listing(){ for (size,wire) in [(0x2,b\"2\")] {} }",
        "fn remarks_are_sliced_without_wire_paging(){ for (size,wire) in [(PAGE_SIZE-1,b\"1\")] {} }",
        "fn remarks_are_sliced_without_wire_paging(){ let _=&REMARKS[..(2usize)]; }",
        "fn remarks_are_sliced_without_wire_paging(){ let _=&REMARKS[..PAGE_SIZE-0x1]; }",
    ];
    for source in bad {
        assert_eq!(check(source), vec![RULE], "negative: {source}");
    }
    println!("listing-sizes controls: 4 positive, 7 negative passed");
}
