//! `smllm compile`: the validated model as compact JSON, prompts inlined, for
//! `smllm-wasm` hosts (CLI-13).

use smllm_core::model::Config;

use crate::finding::Findings;
use crate::load::{ConfigFile, Origin, load_configs, load_machine};

/// Compile a `config.toml` (every machine it lists) or one machine file.
// @zen-impl: CLI-13_AC-1
pub fn compile(path: &std::path::Path) -> (Option<String>, Findings) {
    let is_toml = path.extension().is_some_and(|e| e == "toml");
    let (config, findings) = if is_toml {
        let loaded = load_configs(
            &[ConfigFile {
                path: path.to_path_buf(),
                origin: Origin::Explicit,
            }],
            true,
        );
        (Some(loaded.config), loaded.findings)
    } else {
        let (m, f) = load_machine(path, true);
        (
            m.map(|m| Config {
                machines: vec![m],
                idle: Vec::new(),
            }),
            f,
        )
    };
    if findings.has_errors() {
        return (None, findings);
    }
    (
        config.and_then(|c| serde_json::to_string(&c).ok()),
        findings,
    )
}
