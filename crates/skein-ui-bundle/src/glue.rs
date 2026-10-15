use std::path::Path;
use wasm_bindgen_cli_support::Bindgen;

use crate::{Glue, Refusal};

/// Generate web-target ES-module glue with the pinned glue library.
pub fn glue(module: &Path, name: &str) -> Result<Glue, Refusal> {
    let mut bindgen = Bindgen::new();
    bindgen.input_path(module).out_name(name).omit_default_module_path(false);
    bindgen.web(true).map_err(|_| Refusal::Glue)?;
    let mut output = bindgen.generate_output().map_err(|_| Refusal::Glue)?;
    let mut snippets = Vec::new();
    for (identifier, sources) in output.snippets() {
        for (index, source) in sources.iter().enumerate() {
            snippets.push((format!("{identifier}/inline{index}.js"), source.as_bytes().to_vec()));
        }
    }
    for (path, source) in output.local_modules() {
        snippets.push((path.clone(), source.as_bytes().to_vec()));
    }
    snippets.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(Glue { js: output.js().as_bytes().to_vec(), wasm: output.wasm_mut().emit_wasm(), snippets })
}
