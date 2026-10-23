//! Table-driven changes, spellings and refusals (codec.md, 10.6).
#[cfg(test)]
mod tests {
    use skein_codec::schema;
    use skein_codegen::diff::{Diff, DiffError, Kind, Rename, Version, diff};

    struct Row<'a> {
        name: &'a str,
        kept: &'a str,
        current: &'a str,
        renames: Vec<Rename>,
        expected: &'a str,
    }

    fn declaration(old: &str, new: &str) -> Rename {
        Rename::Declaration { old: old.into(), new: new.into() }
    }

    fn part(decl: &str, old: &str, new: &str) -> Rename {
        Rename::Part { decl: decl.into(), old: old.into(), new: new.into() }
    }

    fn compare(kept: &str, current: &str, renames: &[Rename]) -> Result<Diff, DiffError> {
        let kept = schema::parse(kept.as_bytes()).expect("kept fixture parses");
        let current = schema::parse(current.as_bytes()).expect("current fixture parses");
        diff(&kept, &current, renames)
    }

    fn check(rows: &[Row<'_>]) {
        for row in rows {
            let result = compare(row.kept, row.current, &row.renames).expect(row.name);
            assert_eq!(result.to_string(), row.expected, "{}", row.name);
        }
    }
    #[test]
    #[expect(clippy::too_many_lines, reason = "codec.md 10.6 groups small table rows into one test process")]
    fn declarations_match_before_layouts_are_compared() {
        check(&[
            Row {
                name: "added",
                kept: "family task-store 1\n",
                current: "family task-store 1\nrecord A {}",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone add A\n",
            },
            Row {
                name: "dropped",
                kept: "family task-store 1\nrecord A {}",
                current: "family task-store 1\n",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone drop A\n",
            },
            Row {
                name: "confirmed",
                kept: "family task-store 1\nrecord A {}",
                current: "family task-store 1\nrecord B {}",
                renames: vec![declaration("A", "B")],
                expected: "family task-store 1 to 1\nnone rename A B\n",
            },
            Row {
                name: "proposed",
                kept: "family task-store 1\nrecord A {}",
                current: "family task-store 1\nrecord B {}",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone add B\nnone drop A\nproposed rename A B\n",
            },
            Row {
                name: "not proposed",
                kept: "family task-store 1\nrecord A { x: u8 }",
                current: "family task-store 1\nrecord B { x: u16 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone add B\nnone drop A\n",
            },
            Row {
                name: "reshaped",
                kept: "family task-store 1\nrecord A {}",
                current: "family task-store 1\nenum A { x }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten reshape A\n",
            },
            Row {
                name: "enum reshaped",
                kept: "family task-store 1\nenum A { x }",
                current: "family task-store 1\nrecord A {}",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten reshape A\n",
            },
            Row {
                name: "versioned",
                kept: "family task-store 1\nrecord A {}",
                current: "family task-store 1\nversioned record A {}",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived versioned A\n",
            },
            Row {
                name: "unversioned",
                kept: "family task-store 1\nversioned record A {}",
                current: "family task-store 1\nrecord A {}",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived unversioned A\n",
            },
            Row {
                name: "declaration order",
                kept: "family task-store 1\nrecord A {} record B {}",
                current: "family task-store 1\nrecord B {} record A {}",
                renames: vec![],
                expected: "family task-store 1 to 1\n",
            },
            Row {
                name: "boxed",
                kept: "family task-store 1\nrecord A {} enum E { a: A }",
                current: "family task-store 1\nrecord A {} enum E { a: boxed A }",
                renames: vec![],
                expected: "family task-store 1 to 1\n",
            },
            Row {
                name: "unboxed",
                kept: "family task-store 1\nrecord A {} enum E { a: boxed A }",
                current: "family task-store 1\nrecord A {} enum E { a: A }",
                renames: vec![],
                expected: "family task-store 1 to 1\n",
            },
            Row {
                name: "layout and comments",
                kept: "family task-store 1\nrecord A { x: u8 }",
                current: "family task-store 1\n\n# comment\nrecord A {\n x: u8,\n}\n",
                renames: vec![],
                expected: "family task-store 1 to 1\n",
            },
            Row {
                name: "renames precede name matching",
                kept: "family task-store 1\nrecord A { a: u8 } record B { b: u16 }",
                current: "family task-store 1\nrecord A { b: u16 } record B { a: u8 }",
                renames: vec![declaration("A", "B"), declaration("B", "A")],
                expected: "family task-store 1 to 1\nnone rename B A\nnone rename A B\n",
            },
            Row {
                name: "positional proposals do not search",
                kept: "family task-store 1\nrecord A { x: u8 } record B { x: u16 }",
                current: "family task-store 1\nrecord C { x: u16 } record D { x: u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone add C\nnone add D\nnone drop A\nnone drop B\n",
            },
            Row {
                name: "versioned proposals differ",
                kept: "family task-store 1\nrecord A {}",
                current: "family task-store 1\nversioned record B {}",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone add B\nnone drop A\n",
            },
            Row {
                name: "proposal declarations before parts",
                kept: "family task-store 1\nrecord Gone {} record A { x: u8 }",
                current: "family task-store 1\nrecord A { y: u8 } record New {}",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived add A.y u8 ?\nderived drop A.x\nnone add New\nnone drop Gone\nproposed rename Gone New\nproposed rename A.x y\n",
            },
            Row {
                name: "enum proposal ignores names and boxed",
                kept: "family task-store 1\nrecord R {} enum A { a: R b }",
                current: "family task-store 1\nrecord R {} enum B { x: boxed R y }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone add B\nnone drop A\nproposed rename A B\n",
            },
            Row {
                name: "enum proposal rejects payload mismatch",
                kept: "family task-store 1\nrecord R {} record S {} enum A { a: R }",
                current: "family task-store 1\nrecord R {} record S {} enum B { b: S }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone add B\nnone drop A\n",
            },
            Row {
                name: "proposals compare matched named types",
                kept: "family task-store 1\nrecord R {} record A { x: R }",
                current: "family task-store 1\nrecord S {} record B { y: S }",
                renames: vec![declaration("R", "S")],
                expected: "family task-store 1 to 1\nnone rename R S\nnone add B\nnone drop A\nproposed rename A B\n",
            },
            Row {
                name: "proposal type equal after target changed",
                kept: "family task-store 1\nrecord R { x: u8 } record A { before: R }",
                current: "family task-store 1\nrecord R { x: u16 } record A { after: R }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived widen R.x u8 u16\nderived add A.after R ?\nderived drop A.before\nproposed rename A.before after\n",
            },
        ]);
    }
    #[test]
    #[expect(clippy::too_many_lines, reason = "codec.md 10.6 groups the type comparison table into one test process")]
    fn fields_compare_types_and_preserve_current_order() {
        check(&[
            Row {
                name: "add fields",
                kept: "family task-store 1\nrecord A {}",
                current: "family task-store 1\nrecord A { count: u32 note: option text 64 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived add A.count u32 ?\nderived add A.note option text 64 none\n",
            },
            Row {
                name: "drop field",
                kept: "family task-store 1\nrecord A { x: u32 }",
                current: "family task-store 1\nrecord A {}",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived drop A.x\n",
            },
            Row {
                name: "rename field",
                kept: "family task-store 1\nrecord A { x: u8 }",
                current: "family task-store 1\nrecord A { y: u8 }",
                renames: vec![part("A", "x", "y")],
                expected: "family task-store 1 to 1\nnone rename A.x y\n",
            },
            Row {
                name: "propose field",
                kept: "family task-store 1\nrecord A { x: u8 }",
                current: "family task-store 1\nrecord A { y: u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived add A.y u8 ?\nderived drop A.x\nproposed rename A.x y\n",
            },
            Row {
                name: "rename and widen",
                kept: "family task-store 1\nrecord A { x: u8 }",
                current: "family task-store 1\nrecord A { y: u32 }",
                renames: vec![part("A", "x", "y")],
                expected: "family task-store 1 to 1\nnone rename A.x y\nderived widen A.y u8 u32\n",
            },
            Row {
                name: "field order",
                kept: "family task-store 1\nrecord A { x: u8 y: u16 }",
                current: "family task-store 1\nrecord A { y: u16 x: u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived order A y x\n",
            },
            Row {
                name: "middle addition",
                kept: "family task-store 1\nrecord A { x: u8 y: u16 }",
                current: "family task-store 1\nrecord A { x: u8 z: bool y: u16 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived add A.z bool ?\n",
            },
            Row {
                name: "named changed target",
                kept: "family task-store 1\nrecord B {} record C {} record A { x: B }",
                current: "family task-store 1\nrecord B {} record C {} record A { x: C }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x B C\n",
            },
            Row {
                name: "carried target change",
                kept: "family task-store 1\nrecord B { x: u8 } record A { x: B xs: list 8 B opt: option B data: bytes 64 pieces B } enum E { b: B }",
                current: "family task-store 1\nrecord B { x: u32 } record A { x: B xs: list 8 B opt: option B data: bytes 64 pieces B } enum E { b: B }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived widen B.x u8 u32\n",
            },
            Row {
                name: "renamed pieces",
                kept: "family task-store 1\nrecord B {} record A { x: bytes 64 pieces B }",
                current: "family task-store 1\nrecord C {} record A { x: bytes 64 pieces C }",
                renames: vec![declaration("B", "C")],
                expected: "family task-store 1 to 1\nnone rename B C\n",
            },
            Row {
                name: "retargeted pieces",
                kept: "family task-store 1\nrecord B {} record C {} record A { x: bytes 64 pieces B }",
                current: "family task-store 1\nrecord B {} record C {} record A { x: bytes 64 pieces C }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x bytes 64 pieces B bytes 64 pieces C\n",
            },
            Row {
                name: "renamed declaration and part",
                kept: "family task-store 1\nrecord B { x: u8 }",
                current: "family task-store 1\nrecord C { y: u32 }",
                renames: vec![part("C", "x", "y"), declaration("B", "C")],
                expected: "family task-store 1 to 1\nnone rename B C\nnone rename C.x y\nderived widen C.y u8 u32\n",
            },
            Row {
                name: "integer widen",
                kept: "family task-store 1\nrecord A { x: u8 }",
                current: "family task-store 1\nrecord A { x: u32 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived widen A.x u8 u32\n",
            },
            Row {
                name: "integer equal",
                kept: "family task-store 1\nrecord A { x: u32 }",
                current: "family task-store 1\nrecord A { x: u32 }",
                renames: vec![],
                expected: "family task-store 1 to 1\n",
            },
            Row {
                name: "integer narrow",
                kept: "family task-store 1\nrecord A { x: u32 }",
                current: "family task-store 1\nrecord A { x: u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x u32 u8\n",
            },
            Row {
                name: "text bound widen",
                kept: "family task-store 1\nrecord A { x: text 64 }",
                current: "family task-store 1\nrecord A { x: text 128 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone widen A.x text 64 text 128\n",
            },
            Row {
                name: "text bound narrow",
                kept: "family task-store 1\nrecord A { x: text 128 }",
                current: "family task-store 1\nrecord A { x: text 64 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x text 128 text 64\n",
            },
            Row {
                name: "list integer widen",
                kept: "family task-store 1\nrecord A { x: list 8 u8 }",
                current: "family task-store 1\nrecord A { x: list 8 u32 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived widen A.x list 8 u8 list 8 u32\n",
            },
            Row {
                name: "option integer widen",
                kept: "family task-store 1\nrecord A { x: option u8 }",
                current: "family task-store 1\nrecord A { x: option u32 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived widen A.x option u8 option u32\n",
            },
            Row {
                name: "list bound widen",
                kept: "family task-store 1\nrecord A { x: list 8 u8 }",
                current: "family task-store 1\nrecord A { x: list 16 u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone widen A.x list 8 u8 list 16 u8\n",
            },
            Row {
                name: "list bound narrow",
                kept: "family task-store 1\nrecord A { x: list 8 u8 }",
                current: "family task-store 1\nrecord A { x: list 4 u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x list 8 u8 list 4 u8\n",
            },
            Row {
                name: "text bytes",
                kept: "family task-store 1\nrecord A { x: text 64 }",
                current: "family task-store 1\nrecord A { x: bytes 64 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone widen A.x text 64 bytes 64\n",
            },
            Row {
                name: "bytes text",
                kept: "family task-store 1\nrecord A { x: bytes 64 }",
                current: "family task-store 1\nrecord A { x: text 64 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x bytes 64 text 64\n",
            },
            Row {
                name: "text secret",
                kept: "family task-store 1\nrecord A { x: text 64 }",
                current: "family task-store 1\nrecord A { x: secret 64 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone widen A.x text 64 secret 64\n",
            },
            Row {
                name: "secret text",
                kept: "family task-store 1\nrecord A { x: secret 64 }",
                current: "family task-store 1\nrecord A { x: text 64 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x secret 64 text 64\n",
            },
            Row {
                name: "bytes secret",
                kept: "family task-store 1\nrecord A { x: bytes 64 }",
                current: "family task-store 1\nrecord A { x: secret 64 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone widen A.x bytes 64 secret 64\n",
            },
            Row {
                name: "secret bytes",
                kept: "family task-store 1\nrecord A { x: secret 64 }",
                current: "family task-store 1\nrecord A { x: bytes 64 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone widen A.x secret 64 bytes 64\n",
            },
            Row {
                name: "holds added",
                kept: "family task-store 1\nrecord A { x: bytes 64 }",
                current: "family task-store 1\nrecord A { x: bytes 64 holds other-family Value }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone widen A.x bytes 64 bytes 64 holds other-family Value\n",
            },
            Row {
                name: "holds removed",
                kept: "family task-store 1\nrecord A { x: bytes 64 holds other-family Value }",
                current: "family task-store 1\nrecord A { x: bytes 64 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone widen A.x bytes 64 holds other-family Value bytes 64\n",
            },
            Row {
                name: "holds equal",
                kept: "family task-store 1\nrecord A { x: bytes 64 holds other-family Value }",
                current: "family task-store 1\nrecord A { x: bytes 64 holds other-family Value }",
                renames: vec![],
                expected: "family task-store 1 to 1\n",
            },
            Row {
                name: "holds family retargeted",
                kept: "family task-store 1\nrecord A { x: bytes 64 holds other-family Value }",
                current: "family task-store 1\nrecord A { x: bytes 64 holds third-family Value }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x bytes 64 holds other-family Value bytes 64 holds third-family Value\n",
            },
            Row {
                name: "holds name retargeted",
                kept: "family task-store 1\nrecord A { x: bytes 64 holds other-family Value }",
                current: "family task-store 1\nrecord A { x: bytes 64 holds other-family Other }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x bytes 64 holds other-family Value bytes 64 holds other-family Other\n",
            },
            Row {
                name: "outer wrap",
                kept: "family task-store 1\nrecord A { x: u8 }",
                current: "family task-store 1\nrecord A { x: option u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived wrap A.x u8 option u8\n",
            },
            Row {
                name: "item wrap",
                kept: "family task-store 1\nrecord A { x: list 8 u8 }",
                current: "family task-store 1\nrecord A { x: list 8 option u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived wrap A.x list 8 u8 list 8 option u8\n",
            },
            Row {
                name: "inner option wrap",
                kept: "family task-store 1\nrecord A { x: option u8 }",
                current: "family task-store 1\nrecord A { x: option option u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived wrap A.x option u8 option option u8\n",
            },
            Row {
                name: "multiple wrap and widen",
                kept: "family task-store 1\nrecord A { x: list 8 u8 }",
                current: "family task-store 1\nrecord A { x: option list 16 option u32 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived wrap A.x list 8 u8 option list 16 option u32\n",
            },
            Row {
                name: "option removed",
                kept: "family task-store 1\nrecord A { x: option u8 }",
                current: "family task-store 1\nrecord A { x: u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x option u8 u8\n",
            },
            Row {
                name: "option removed inside",
                kept: "family task-store 1\nrecord A { x: list 8 option u8 }",
                current: "family task-store 1\nrecord A { x: list 8 u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x list 8 option u8 list 8 u8\n",
            },
            Row {
                name: "list added",
                kept: "family task-store 1\nrecord A { x: u8 }",
                current: "family task-store 1\nrecord A { x: list 8 u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x u8 list 8 u8\n",
            },
            Row {
                name: "list removed",
                kept: "family task-store 1\nrecord A { x: list 8 u8 }",
                current: "family task-store 1\nrecord A { x: u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x list 8 u8 u8\n",
            },
            Row {
                name: "list option",
                kept: "family task-store 1\nrecord A { x: list 8 u8 }",
                current: "family task-store 1\nrecord A { x: option u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x list 8 u8 option u8\n",
            },
            Row {
                name: "option list",
                kept: "family task-store 1\nrecord A { x: option u8 }",
                current: "family task-store 1\nrecord A { x: list 8 u8 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x option u8 list 8 u8\n",
            },
            Row {
                name: "duration",
                kept: "family task-store 1\nrecord A { x: u64 }",
                current: "family task-store 1\nrecord A { x: duration }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x u64 duration\n",
            },
            Row {
                name: "duration back",
                kept: "family task-store 1\nrecord A { x: duration }",
                current: "family task-store 1\nrecord A { x: u64 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x duration u64\n",
            },
            Row {
                name: "fixed",
                kept: "family task-store 1\nrecord A { x: fixed 16 }",
                current: "family task-store 1\nrecord A { x: fixed 32 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x fixed 16 fixed 32\n",
            },
            Row {
                name: "bool",
                kept: "family task-store 1\nrecord A { x: u8 }",
                current: "family task-store 1\nrecord A { x: bool }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x u8 bool\n",
            },
            Row {
                name: "wrap narrow",
                kept: "family task-store 1\nrecord A { x: text 64 }",
                current: "family task-store 1\nrecord A { x: option text 32 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x text 64 option text 32\n",
            },
            Row {
                name: "bytes bound widen",
                kept: "family task-store 1\nrecord A { x: bytes 64 }",
                current: "family task-store 1\nrecord A { x: bytes 128 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone widen A.x bytes 64 bytes 128\n",
            },
            Row {
                name: "bytes bound narrow",
                kept: "family task-store 1\nrecord A { x: bytes 64 }",
                current: "family task-store 1\nrecord A { x: bytes 32 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x bytes 64 bytes 32\n",
            },
            Row {
                name: "unmatched same-index named types",
                kept: "family task-store 1\nrecord B {} record A { x: B }",
                current: "family task-store 1\nrecord C {} record A { x: C }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone add C\nwritten retype A.x B C\nnone drop B\nproposed rename B C\n",
            },
            Row {
                name: "kept and current type names spelled separately",
                kept: "family task-store 1\nrecord B {} record A { x: B }",
                current: "family task-store 1\nrecord C {} record A { x: option C }",
                renames: vec![declaration("B", "C")],
                expected: "family task-store 1 to 1\nnone rename B C\nderived wrap A.x B option C\n",
            },
            Row {
                name: "part renames in independent scopes",
                kept: "family task-store 1\nrecord A { x: u8 } record B { x: u8 }",
                current: "family task-store 1\nrecord A { y: u8 } record B { y: u8 }",
                renames: vec![part("A", "x", "y"), part("B", "x", "y")],
                expected: "family task-store 1 to 1\nnone rename A.x y\nnone rename B.x y\n",
            },
            Row {
                name: "rename order uses kept positions",
                kept: "family task-store 1\nrecord A { x: u8 y: u16 z: bool }",
                current: "family task-store 1\nrecord A { last: bool middle: u8 first: u16 }",
                renames: vec![part("A", "x", "middle"), part("A", "y", "first"), part("A", "z", "last")],
                expected: "family task-store 1 to 1\nnone rename A.z last\nnone rename A.x middle\nnone rename A.y first\nderived order A last middle first\n",
            },
            Row {
                name: "pieces added and removed",
                kept: "family task-store 1\nrecord B {} record A { x: bytes 64 y: bytes 64 pieces B }",
                current: "family task-store 1\nrecord B {} record A { x: bytes 64 pieces B y: bytes 64 }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone widen A.x bytes 64 bytes 64 pieces B\nnone widen A.y bytes 64 pieces B bytes 64\n",
            },
            Row {
                name: "holds and pieces cannot replace each other",
                kept: "family task-store 1\nrecord B {} record A { x: bytes 64 holds other-family Value y: bytes 64 pieces B }",
                current: "family task-store 1\nrecord B {} record A { x: bytes 64 pieces B y: bytes 64 holds other-family Value }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten retype A.x bytes 64 holds other-family Value bytes 64 pieces B\nwritten retype A.y bytes 64 pieces B bytes 64 holds other-family Value\n",
            },
            Row {
                name: "equal leaves and wrapper chains",
                kept: "family task-store 1\nrecord A { a: u8 b: u16 c: u32 d: u64 e: bool f: duration g: fixed 16 h: secret 64 i: text 64 j: bytes 64 k: list 8 option u16 }",
                current: "family task-store 1\nrecord A { a: u8 b: u16 c: u32 d: u64 e: bool f: duration g: fixed 16 h: secret 64 i: text 64 j: bytes 64 k: list 8 option u16 }",
                renames: vec![],
                expected: "family task-store 1 to 1\n",
            },
        ]);
    }
    #[test]
    fn variants_compare_tags_payloads_and_confirmed_names() {
        check(&[
            Row {
                name: "append",
                kept: "family task-store 1\nenum E { a }",
                current: "family task-store 1\nenum E { a b }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone add E.b\n",
            },
            Row {
                name: "insert",
                kept: "family task-store 1\nenum E { a }",
                current: "family task-store 1\nenum E { b a }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived add E.b\n",
            },
            Row {
                name: "reorder",
                kept: "family task-store 1\nenum E { a b }",
                current: "family task-store 1\nenum E { b a }",
                renames: vec![],
                expected: "family task-store 1 to 1\nderived order E b a\n",
            },
            Row {
                name: "rename",
                kept: "family task-store 1\nenum E { a }",
                current: "family task-store 1\nenum E { b }",
                renames: vec![part("E", "a", "b")],
                expected: "family task-store 1 to 1\nnone rename E.a b\n",
            },
            Row {
                name: "propose",
                kept: "family task-store 1\nenum E { a }",
                current: "family task-store 1\nenum E { b }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone add E.b\nwritten drop E.a\nproposed rename E.a b\n",
            },
            Row {
                name: "drop",
                kept: "family task-store 1\nenum E { a b }",
                current: "family task-store 1\nenum E { a }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten drop E.b\n",
            },
            Row {
                name: "payload add",
                kept: "family task-store 1\nrecord R {} enum E { a }",
                current: "family task-store 1\nrecord R {} enum E { a: R }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten payload E.a - R\n",
            },
            Row {
                name: "payload remove",
                kept: "family task-store 1\nrecord R {} enum E { a: R }",
                current: "family task-store 1\nrecord R {} enum E { a }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten payload E.a R -\n",
            },
            Row {
                name: "payload change",
                kept: "family task-store 1\nrecord R {} record S {} enum E { a: R }",
                current: "family task-store 1\nrecord R {} record S {} enum E { a: S }",
                renames: vec![],
                expected: "family task-store 1 to 1\nwritten payload E.a R S\n",
            },
            Row {
                name: "payload renamed",
                kept: "family task-store 1\nrecord R {} enum E { a: R }",
                current: "family task-store 1\nrecord S {} enum E { a: S }",
                renames: vec![declaration("R", "S")],
                expected: "family task-store 1 to 1\nnone rename R S\n",
            },
            Row {
                name: "payload prevents proposal",
                kept: "family task-store 1\nrecord R {} enum E { a }",
                current: "family task-store 1\nrecord R {} enum E { b: R }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone add E.b\nwritten drop E.a\n",
            },
            Row {
                name: "renamed matched variant counts after insertion",
                kept: "family task-store 1\nenum E { a b }",
                current: "family task-store 1\nenum E { c renamed }",
                renames: vec![part("E", "b", "renamed")],
                expected: "family task-store 1 to 1\nderived add E.c\nnone rename E.b renamed\nwritten drop E.a\nproposed rename E.a c\n",
            },
            Row {
                name: "renamed payload changes use current variant name",
                kept: "family task-store 1\nrecord R {} enum E { a }",
                current: "family task-store 1\nrecord R {} enum E { b: R }",
                renames: vec![part("E", "a", "b")],
                expected: "family task-store 1 to 1\nnone rename E.a b\nwritten payload E.b - R\n",
            },
            Row {
                name: "append follows last surviving variant",
                kept: "family task-store 1\nenum E { a b }",
                current: "family task-store 1\nenum E { a c d }",
                renames: vec![],
                expected: "family task-store 1 to 1\nnone add E.c\nnone add E.d\nwritten drop E.b\nproposed rename E.b c\n",
            },
        ]);
    }

    #[test]
    fn versions_follow_the_highest_kind_without_overflow() {
        let rows = [
            ("kept", 1, 1_u16, "u8", "u8", None, Version::Kept),
            ("moved", 1, 2, "u8", "u16", Some(Kind::Derived), Version::Moved),
            ("unmoved", 1, 1, "u8", "u16", Some(Kind::Derived), Version::Unmoved),
            ("needless", 1, 2, "u8", "u8", None, Version::Needless),
            ("wrong gap", 1, 3, "u8", "u16", Some(Kind::Derived), Version::Wrong),
            ("wrong backwards", 2, 1, "u8", "u8", None, Version::Wrong),
            ("maximum cannot move", u16::MAX, 0, "u8", "u16", Some(Kind::Derived), Version::Wrong),
            ("none needed", 1, 1, "text 8", "text 16", Some(Kind::NoneNeeded), Version::Kept),
            ("written moved", 1, 2, "u16", "u8", Some(Kind::Written), Version::Moved),
        ];
        for (name, k, c, kt, ct, kind, version) in rows {
            let kept = format!("family f {k} versioned record A {{ x: {kt} }}");
            let current = format!("family f {c} versioned record A {{ x: {ct} }}");
            let result = compare(&kept, &current, &[]).expect(name);
            assert_eq!(result.kind(), kind, "{name}");
            assert_eq!(result.version(), version, "{name}");
        }
    }

    #[test]
    fn written_sites_are_distinct_in_current_declaration_order() {
        let kept = "family f 1 record A { x: u32 y: u32 } enum B { a b } record C {}";
        let current = "family f 2 enum B { a } enum C { a } record A { x: u8 y: u8 }";
        let result = compare(kept, current, &[]).expect("same family");
        assert_eq!(result.written(), ["B", "C", "A"]);
        assert_eq!(
            result.to_string(),
            "family f 1 to 2\nwritten drop B.b\nwritten reshape C\nwritten retype A.x u32 u8\nwritten retype A.y u32 u8\n"
        );
    }

    #[test]
    fn invalid_renames_report_the_first_offending_input_index() {
        let rows = [
            ("family", "family other 1", "family f 1", vec![], DiffError::Family),
            (
                "unknown declaration old",
                "record A {}",
                "record B {}",
                vec![declaration("Z", "B")],
                DiffError::Rename(0),
            ),
            (
                "unknown declaration new",
                "record A {}",
                "record B {}",
                vec![declaration("A", "Z")],
                DiffError::Rename(0),
            ),
            (
                "declaration old twice",
                "record A {}",
                "record B {} record C {}",
                vec![declaration("A", "B"), declaration("A", "C")],
                DiffError::Rename(1),
            ),
            (
                "declaration new twice",
                "record A {} record B {}",
                "record C {}",
                vec![declaration("A", "C"), declaration("B", "C")],
                DiffError::Rename(1),
            ),
            (
                "unknown part old",
                "record A { x: u8 }",
                "record A { y: u8 }",
                vec![part("A", "z", "y")],
                DiffError::Rename(0),
            ),
            (
                "unknown part new",
                "record A { x: u8 }",
                "record A { y: u8 }",
                vec![part("A", "x", "z")],
                DiffError::Rename(0),
            ),
            (
                "unknown scope",
                "record A { x: u8 }",
                "record A { y: u8 }",
                vec![part("Z", "x", "y")],
                DiffError::Rename(0),
            ),
            (
                "unmatched scope",
                "record A { x: u8 }",
                "record B { y: u8 }",
                vec![part("B", "x", "y")],
                DiffError::Rename(0),
            ),
            ("reshaped scope", "record A { x: u8 }", "enum A { y }", vec![part("A", "x", "y")], DiffError::Rename(0)),
            (
                "part old twice",
                "record A { x: u8 }",
                "record A { y: u8 z: u8 }",
                vec![part("A", "x", "y"), part("A", "x", "z")],
                DiffError::Rename(1),
            ),
            (
                "part new twice",
                "enum A { x z }",
                "enum A { y }",
                vec![part("A", "x", "y"), part("A", "z", "y")],
                DiffError::Rename(1),
            ),
            (
                "earlier part error before declaration error",
                "record A {}",
                "record A {}",
                vec![part("A", "x", "y"), declaration("Z", "A")],
                DiffError::Rename(0),
            ),
            (
                "kept scope name refused",
                "record A { x: u8 }",
                "record B { y: u8 }",
                vec![declaration("A", "B"), part("A", "x", "y")],
                DiffError::Rename(1),
            ),
        ];
        for (name, k, c, renames, expected) in rows {
            let kept = if k.starts_with("family") { k.to_owned() } else { format!("family f 1 {k}") };
            let current = if c.starts_with("family") { c.to_owned() } else { format!("family f 1 {c}") };
            assert_eq!(compare(&kept, &current, &renames), Err(expected), "{name}");
        }
    }
}
