//! Starting points written by `smllm new` and `smllm init` (CFG-15).

/// A new `<id>.smllm.yaml`.
pub fn machine_template(id: &str) -> String {
    format!(
        r#"# yaml-language-server: $schema=https://raw.githubusercontent.com/six5536/smllm/main/schema/smllm.schema.json
# An XState v5 machine config (a strict subset); smllm data lives under `meta`.
id: {id}
description: What this state machine is for.
initial: START
meta:
  smllm: 1
  instance:
    kind: task
    ref:
      param: taskId
      description: "The task's id."
  events:
    done:
      description: Select when the work this state asks for is finished.
states:
  START:
    meta: {{ entryPoint: true }}
    entry: {{ type: prompt, params: {{ text: "Describe what the agent should do here." }} }}
    on:
      done: DONE
  DONE:
    type: final
    entry: {{ type: prompt, params: {{ text: "Summarise what was done." }} }}
"#
    )
}

/// A new `config.toml`.
pub fn config_template() -> &'static str {
    r#"# smllm config. Paths are relative to this file.
[machines]
files = []

# [idle]
# on-enter = { file = "idle.md" }   # optional extra text for the idle list
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_template_is_a_valid_machine() {
        let dir = std::env::temp_dir().join(format!("smllm-tpl-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("demo.smllm.yaml");
        std::fs::write(&file, machine_template("demo")).unwrap();
        let (m, findings) = crate::load_machine(&file, false);
        assert!(m.is_some(), "{findings:?}");
        assert!(
            findings.0.iter().all(|f| f.level != crate::Level::Error),
            "{findings:?}"
        );
        let cfg: toml::Table = toml::from_str(config_template()).unwrap();
        assert!(cfg.contains_key("machines"));
        std::fs::remove_dir_all(dir).ok();
    }
}
