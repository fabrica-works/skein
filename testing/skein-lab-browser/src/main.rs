//! The lab's browser: one io loop, one call at a time.
mod calls;
mod config;
mod files;
mod json;
mod proxy;
mod runtime;
#[cfg(test)]
mod tests;
fn main() {
    let config = match config::Config::parse(std::env::args_os().skip(1)) {
        Ok(config) => config,
        Err(usage) => {
            eprintln!("{usage}");
            std::process::exit(2);
        }
    };
    if let Err(error) = runtime::run(&config) {
        eprintln!("skein-lab-browser: {error}");
        std::process::exit(1);
    }
}
