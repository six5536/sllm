//! User directories and config lookup (CLI conventions, STO-1).
// @zen-component: CLI-Lookup

use std::path::{Path, PathBuf};

use etcetera::BaseStrategy;
use smllm_format::{ConfigFile, Origin};

use crate::error::{Error, Result};

/// `.smllm`: the project config directory.
pub const PROJECT_DIR: &str = ".smllm";
/// `config.toml`.
pub const CONFIG_FILE: &str = "config.toml";

fn strategy() -> Result<impl BaseStrategy> {
    etcetera::choose_base_strategy().map_err(|e| Error::msg(format!("no home directory: {e}")))
}

/// `$XDG_CONFIG_HOME/smllm` (`~/.config/smllm`).
pub fn user_config_dir() -> Result<PathBuf> {
    Ok(strategy()?.config_dir().join("smllm"))
}

/// `$XDG_STATE_HOME/smllm` (`~/.local/state/smllm`): sessions and bindings.
pub fn user_state_dir() -> Result<PathBuf> {
    let s = strategy()?;
    Ok(s.state_dir().unwrap_or_else(|| s.data_dir()).join("smllm"))
}

/// `~/.claude`.
pub fn claude_user_dir() -> Result<PathBuf> {
    Ok(strategy()?.home_dir().join(".claude"))
}

/// The nearest `.smllm/config.toml` at or above `from`.
pub fn project_config(from: &Path) -> Option<PathBuf> {
    from.ancestors()
        .map(|d| d.join(PROJECT_DIR).join(CONFIG_FILE))
        .find(|p| p.is_file())
}

/// `--config`, else a non-empty `SMLLM_CONFIG`.
pub fn explicit(flag: Option<&Path>) -> Option<PathBuf> {
    flag.map(Path::to_path_buf).or_else(|| {
        std::env::var_os("SMLLM_CONFIG")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    })
}

/// Configs to load: `--config`/`SMLLM_CONFIG` alone, else the user config
/// then the nearest project config (D26). Empty when none exists (HOST-8).
// @zen-impl: CLI-3_AC-2
pub fn lookup(explicit: Option<&Path>, from: &Path) -> Result<Vec<ConfigFile>> {
    if let Some(p) = self::explicit(explicit) {
        let p = if p.is_relative() { from.join(p) } else { p };
        if !p.is_file() {
            return Err(Error::msg(format!("config {} does not exist", p.display())));
        }
        return Ok(vec![ConfigFile {
            path: p,
            origin: Origin::Explicit,
        }]);
    }
    let mut out = Vec::new();
    let user = user_config_dir()?.join(CONFIG_FILE);
    if user.is_file() {
        out.push(ConfigFile {
            path: user,
            origin: Origin::User,
        });
    }
    if let Some(p) = project_config(from) {
        out.push(ConfigFile {
            path: p,
            origin: Origin::Project,
        });
    }
    Ok(out)
}

/// The configs recorded on a session (STO-2), as `ConfigFile`s.
pub fn recorded(paths: &[String]) -> Vec<ConfigFile> {
    paths
        .iter()
        .map(|p| ConfigFile {
            path: PathBuf::from(p),
            origin: Origin::Explicit,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_config_is_found_upward() {
        let d = std::env::temp_dir().join(format!("smllm-paths-{}", std::process::id()));
        let deep = d.join("a/b");
        std::fs::create_dir_all(&deep).unwrap();
        std::fs::create_dir_all(d.join(PROJECT_DIR)).unwrap();
        std::fs::write(d.join(PROJECT_DIR).join(CONFIG_FILE), "").unwrap();
        assert_eq!(project_config(&deep), Some(d.join(".smllm/config.toml")));
        let found = lookup(Some(Path::new("nope.toml")), &deep);
        assert!(found.is_err());
        let found = lookup(Some(&d.join(".smllm/config.toml")), &deep).unwrap();
        assert_eq!(found[0].origin, Origin::Explicit);
        std::fs::remove_dir_all(d).ok();
    }
}
