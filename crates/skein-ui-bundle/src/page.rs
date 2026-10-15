use std::fmt::Write;
use std::fs;

use crate::{Digest, Inputs, Refusal, io};

pub(crate) fn starter(name: &str) -> Vec<u8> {
    format!("import init from \"./{name}.js\";\ninit();\n").into_bytes()
}

fn escape(text: &str) -> String {
    text.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&#39;")
}

pub(crate) fn render(inputs: &Inputs, digest: Digest) -> Result<Vec<u8>, Refusal> {
    let directory = format!("{}{digest}/", inputs.prefix);
    if let Some(path) = &inputs.page {
        let bytes = io(path, fs::read(path))?;
        return replace_marker(&bytes, directory.as_bytes()).ok_or_else(|| Refusal::NoMarker { page: path.clone() });
    }
    let mut page = format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{}</title>\n",
        escape(&inputs.name)
    );
    for name in std::iter::once("skein-ui.css").chain(
        inputs
            .stylesheets
            .iter()
            .map(|path| path.file_name().and_then(|name| name.to_str()).expect("bundle checked stylesheet name")),
    ) {
        writeln!(page, "<link rel=\"stylesheet\" href=\"{}{}\">", escape(&directory), escape(name))
            .expect("write page");
    }
    writeln!(page, "<script type=\"module\" src=\"{}boot.js\"></script>", escape(&directory)).expect("write page");
    write!(
        page,
        "</head>\n<body>\n<div id=\"app\"></div>\n<noscript>{}</noscript>\n</body>\n</html>\n",
        escape(&inputs.noscript)
    )
    .expect("write page");
    Ok(page.into_bytes())
}

fn replace_marker(bytes: &[u8], directory: &[u8]) -> Option<Vec<u8>> {
    const MARKER: &[u8] = b"{bundle}";
    let mut remaining = bytes;
    let mut page = Vec::new();
    let mut found = false;
    while let Some(index) = remaining.windows(MARKER.len()).position(|part| part == MARKER) {
        page.extend_from_slice(&remaining[..index]);
        page.extend_from_slice(directory);
        remaining = &remaining[index + MARKER.len()..];
        found = true;
    }
    page.extend_from_slice(remaining);
    found.then_some(page)
}
