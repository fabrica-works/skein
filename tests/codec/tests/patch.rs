//! The every fixture's patch story (codec.md, 11.8).

#[cfg(test)]
mod tests {
    use std::fs;

    use skein_codec::schema;
    use skein_codegen::diff::{Op, Rename, diff};

    #[test]
    fn patch_operations_spell_the_diff_in_order() {
        let kept = fs::read("schema/every-v1.schema").expect("version 1 readable");
        let current = fs::read("schema/every-v2.schema").expect("version 2 readable");
        let kept = schema::parse(&kept).expect("version 1 valid");
        let current = schema::parse(&current).expect("version 2 valid");
        let patch = fs::read_to_string("patches/0001.patch").expect("patch readable");
        let lines = patch
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter(|line| !line.starts_with("patch ") && !line.starts_with("owner ") && !line.starts_with("family "))
            .collect::<Vec<_>>();
        let renames = lines
            .iter()
            .filter_map(|line| line.strip_prefix("rename "))
            .map(|line| {
                let (old, new) = line.split_once(' ').expect("rename's old and new names");
                if let Some((decl, old)) = old.split_once('.') {
                    Rename::Part { decl: decl.into(), old: old.into(), new: new.into() }
                } else {
                    Rename::Declaration { old: old.into(), new: new.into() }
                }
            })
            .collect::<Vec<_>>();
        let result = diff(&kept, &current, &renames).expect("same family and valid renames");
        let (sites, operations): (Vec<_>, Vec<_>) = lines.into_iter().partition(|line| line.starts_with("written "));
        assert_eq!(sites, ["written Shape", "written Every.score"]);
        assert_eq!(operations.len(), result.changes.len());
        for (line, change) in operations.into_iter().zip(result.changes) {
            if let Op::AddField { decl, field, ty, .. } = &change.op {
                let prefix = format!("add {decl}.{field} {ty} ");
                let value = line.strip_prefix(&prefix).expect("added field spells the diff up to its value");
                assert!(!value.is_empty(), "{line}: added field has a value");
            } else {
                assert_eq!(line, change.to_string());
            }
        }
    }

    // Planning and running version 1's goldens go here, with results in patches/golden/0001/.
}
