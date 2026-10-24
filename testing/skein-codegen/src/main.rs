//! Command line entry point for codec.md, sections 6 and 12.8.

extern crate alloc;

use alloc::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::process::{Command, ExitCode};

use skein_codegen::diff::Rename;
use skein_codegen::patch::{self, Landed, Options};
use skein_codegen::{generate, parse};

const USAGE: &str = "usage: skein-codegen <schema> <out-dir> | patch <crate> [--baseline] [--owner <name>] [--after <upstream> <level>] [--rename <family> <old> <new>] [--base <ref>] [--check]";

#[derive(Debug, PartialEq, Eq)]
enum Arguments {
    Generate { schema: PathBuf, out: PathBuf },
    Patch { owner: PathBuf, options: Options, base: Option<String>, check: bool },
}

fn arguments(args: &[String]) -> Result<Arguments, &'static str> {
    let mut args = args.iter();
    let first = args.next().ok_or(USAGE)?;
    let second = args.next().ok_or(USAGE)?;
    if first != "patch" {
        return if args.next().is_none() {
            Ok(Arguments::Generate { schema: first.into(), out: second.into() })
        } else {
            Err(USAGE)
        };
    }
    let mut options = Options::default();
    let mut base = None;
    let mut check = false;
    while let Some(option) = args.next() {
        match option.as_str() {
            "--baseline" => options.baseline = true,
            "--owner" => options.owner = Some(args.next().ok_or(USAGE)?.clone()),
            "--after" => {
                let upstream = args.next().ok_or(USAGE)?.clone();
                let level = args.next().ok_or(USAGE)?.parse().map_err(|_| USAGE)?;
                options.after.push((upstream, level));
            }
            "--rename" => {
                let family = args.next().ok_or(USAGE)?.clone();
                let old = args.next().ok_or(USAGE)?;
                let new = args.next().ok_or(USAGE)?.clone();
                let rename = if let Some((decl, old)) = old.split_once('.') {
                    Rename::Part { decl: decl.into(), old: old.into(), new }
                } else {
                    Rename::Declaration { old: old.clone(), new }
                };
                options.renames.push((family, rename));
            }
            "--base" => {
                let reference = args.next().ok_or(USAGE)?;
                if reference.starts_with('-') {
                    return Err(USAGE);
                }
                base = Some(reference.clone());
            }
            "--check" => check = true,
            _ => return Err(USAGE),
        }
    }
    Ok(Arguments::Patch { owner: second.into(), options, base, check })
}

#[expect(clippy::print_stderr, reason = "the command's errors are printed to stderr (codec.md 12.8)")]
fn main() -> ExitCode {
    match execute() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(2)
        }
    }
}

#[expect(clippy::print_stdout, reason = "the binary prints the run's report and the check's findings (codec.md 12.8)")]
fn execute() -> Result<u8, Box<dyn core::error::Error>> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match arguments(&args)? {
        Arguments::Generate { schema, out } => {
            generate_files(&schema, &out)?;
            Ok(0)
        }
        Arguments::Patch { owner, options, base, check } => {
            let landed = landed(&owner, base.as_deref())?;
            if check {
                let findings = patch::check(&owner, &landed)?;
                for finding in &findings {
                    println!("{finding}");
                }
                Ok(u8::from(!findings.is_empty()))
            } else {
                let report = patch::run(&owner, &landed, &options)?;
                print!("{report}");
                Ok(u8::from(!report.findings.is_empty()))
            }
        }
    }
}

fn git(owner: &Path, args: &[&str]) -> Result<Vec<u8>, Box<dyn core::error::Error>> {
    let output = Command::new("git").arg("-C").arg(owner).args(args).output()?;
    if !output.status.success() {
        return Err(format!("git {}: {}", args.join(" "), String::from_utf8_lossy(&output.stderr).trim()).into());
    }
    Ok(output.stdout)
}

fn landed(owner: &Path, base: Option<&str>) -> Result<Landed, Box<dyn core::error::Error>> {
    let reference = match base {
        Some(reference) => reference.to_owned(),
        None => String::from_utf8(git(owner, &["merge-base", "HEAD", "origin/main"])?)?.trim().to_owned(),
    };
    let paths = git(owner, &["ls-tree", "-r", "--name-only", "-z", &reference, "--", "patches", "schema/kept"])?;
    let mut files = BTreeMap::new();
    for bytes in paths.split(|byte| *byte == 0).filter(|path| !path.is_empty()) {
        let name = core::str::from_utf8(bytes)?;
        let path = PathBuf::from(name);
        if path.components().any(|part| !matches!(part, Component::Normal(_)))
            || !(path.starts_with("patches") || path.starts_with("schema/kept"))
        {
            return Err(format!("git returned a path outside the crate: {name}").into());
        }
        let object = format!("{reference}:./{name}");
        let bytes = git(owner, &["show", &object])?;
        files.insert(path, bytes);
    }
    Ok(Landed::new(files))
}

fn generate_files(schema_path: &Path, out_dir: &Path) -> Result<(), Box<dyn core::error::Error>> {
    let source = std::fs::read_to_string(schema_path)?;
    let schema = parse(&source)?;
    std::fs::create_dir_all(out_dir)?;
    let output = generate(&schema);
    let path = out_dir.join(format!("v{}.rs", schema.version));
    std::fs::write(path, output.rust)?;
    let schema_directory = schema_path.parent().ok_or("schema has no parent directory")?;
    let crate_directory = schema_directory.parent().ok_or("schema directory has no parent")?;
    let golden_directory = crate_directory.join("golden").join(format!("v{}", schema.version));
    std::fs::create_dir_all(&golden_directory)?;
    for golden in output.goldens {
        std::fs::write(golden_directory.join(golden.name), golden.bytes)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Arguments, USAGE, arguments};
    use skein_codegen::diff::Rename;
    use skein_codegen::patch::Options;

    fn read(args: &[&str]) -> Result<Arguments, &'static str> {
        arguments(&args.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>())
    }

    #[test]
    fn schema_generation_keeps_its_two_positional_arguments() {
        assert_eq!(read(&["schema", "out"]), Ok(Arguments::Generate { schema: "schema".into(), out: "out".into() }));
    }

    #[test]
    fn patch_options_preserve_order_and_both_rename_forms() {
        assert_eq!(
            read(&[
                "patch",
                "crate",
                "--baseline",
                "--owner",
                "shop",
                "--after",
                "first",
                "7",
                "--after",
                "second",
                "3",
                "--rename",
                "shelf",
                "Old",
                "New",
                "--rename",
                "shelf",
                "New.old",
                "new",
                "--base",
                "HEAD",
                "--check"
            ]),
            Ok(Arguments::Patch {
                owner: "crate".into(),
                options: Options {
                    baseline: true,
                    owner: Some("shop".into()),
                    after: vec![("first".into(), 7), ("second".into(), 3)],
                    renames: vec![
                        ("shelf".into(), Rename::Declaration { old: "Old".into(), new: "New".into() }),
                        ("shelf".into(), Rename::Part { decl: "New".into(), old: "old".into(), new: "new".into() })
                    ],
                },
                base: Some("HEAD".into()),
                check: true
            })
        );
    }

    #[test]
    fn patch_without_options_has_pure_defaults() {
        assert_eq!(
            read(&["patch", "crate"]),
            Ok(Arguments::Patch { owner: "crate".into(), options: Options::default(), base: None, check: false })
        );
    }

    #[test]
    fn incomplete_unknown_and_invalid_options_are_usage_errors() {
        for args in [
            vec![],
            vec!["schema"],
            vec!["schema", "out", "extra"],
            vec!["patch", "crate", "--owner"],
            vec!["patch", "crate", "--after", "up", "no"],
            vec!["patch", "crate", "--rename", "shelf", "Old"],
            vec!["patch", "crate", "--base"],
            vec!["patch", "crate", "--base", "--help"],
            vec!["patch", "crate", "--unknown"],
        ] {
            assert_eq!(read(&args), Err(USAGE));
        }
    }
}
