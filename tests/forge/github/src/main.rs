use std::path::PathBuf;
fn main() {
    if let Err(error) = main_result() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
fn main_result() -> github_conformance::Result<()> {
    let mut args = std::env::args_os().skip(1);
    let mut live = None;
    let mut clean = None;
    let mut out = PathBuf::from("github-conformance-out");
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--fake") => live = None,
            Some("--live") => live = Some(PathBuf::from(args.next().ok_or("--live needs a directory")?)),
            Some("--out") => out = PathBuf::from(args.next().ok_or("--out needs a directory")?),
            Some("--clean") => clean = Some(PathBuf::from(args.next().ok_or("--clean needs an output directory")?)),
            Some(_) | None => {
                return Err(
                    "usage: github-conformance [--fake | --live DIRECTORY] [--out DIRECTORY] [--clean OUTPUT]".into()
                );
            }
        }
    }
    if let Some(dir) = clean {
        github_conformance::clean(&dir, live.as_deref())?;
    } else {
        let report = match live {
            Some(dir) => github_conformance::run_live(&dir, &out)?,
            None => github_conformance::run_fake(&out)?,
        };
        println!("{} facts, {} requests; {}", report.facts.len(), report.requests, out.display());
        if report.facts.iter().any(|fact| matches!(fact.outcome, skein_forge_conformance::facts::Outcome::Fails(_))) {
            return Err("conformance recorded failed facts".into());
        }
    }
    Ok(())
}
