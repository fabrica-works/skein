//! Kept/current fixture stories (codec.md, 10.6).
//! Set `SKEIN_DIFF_WRITE=1` to rewrite `diff/<case>/diff.txt` from the schema pairs.

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::Path;

    use skein_codec::schema;
    use skein_codegen::diff::{Kind, Rename, Version, diff};

    #[test]
    fn task_store_changes_tell_each_kind_and_mixed_landing() {
        let rows = [
            (
                "none",
                vec![
                    Rename::Declaration { old: "Task".into(), new: "WorkItem".into() },
                    Rename::Part { decl: "WorkItem".into(), old: "title".into(), new: "summary".into() },
                    Rename::Part { decl: "State".into(), old: "open".into(), new: "pending".into() },
                ],
                Kind::NoneNeeded,
                Version::Kept,
                vec![],
            ),
            ("derived", vec![], Kind::Derived, Version::Moved, vec![]),
            ("written", vec![], Kind::Written, Version::Moved, vec!["Key", "Task", "Event"]),
            ("mixed", vec![], Kind::Written, Version::Moved, vec!["Task"]),
        ];
        let write = std::env::var_os("SKEIN_DIFF_WRITE").is_some_and(|value| value == "1");
        for (case, renames, kind, version, written) in rows {
            let directory = Path::new("diff").join(case);
            let read = |name| fs::read(directory.join(name)).expect("story fixture readable");
            let kept = schema::parse(&read("kept.schema")).expect("valid kept story");
            let current = schema::parse(&read("current.schema")).expect("valid current story");
            let result = diff(&kept, &current, &renames).expect("same family and valid renames");
            assert_eq!(result.kind(), Some(kind), "{case}");
            assert_eq!(result.version(), version, "{case}");
            assert_eq!(result.written(), written, "{case}");
            let path = directory.join("diff.txt");
            if write {
                fs::write(path, result.to_string()).expect("write story diff");
            } else {
                let expected = fs::read_to_string(path).expect("story diff committed");
                assert_eq!(
                    result.to_string(),
                    expected,
                    "{case}: run SKEIN_DIFF_WRITE=1 heavy cargo nextest run -p skein-codec-tests --test diff"
                );
            }
        }
    }
}
