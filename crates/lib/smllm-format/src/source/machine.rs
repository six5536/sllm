//! The state machine file as written: an XState v5 machine config in YAML,
//! smllm data under `meta` (CFG-1, CFG-2). The JSON Schema is derived from
//! these types (CFG-15).
// @zen-component: CFG-Source

use schemars::JsonSchema;
use serde::Deserialize;

use crate::source::{OneOrMany, OrderedMap, Run, StringOr};

/// A state machine file (`<id>.smllm.yaml`).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
#[schemars(
    title = "smllm state machine",
    description = "A strict subset of an XState v5 machine config, written in YAML; smllm data lives under `meta`."
)]
pub struct MachineFile {
    /// Unique within the config.
    pub id: String,
    /// Shown in the idle list.
    #[serde(default)]
    pub description: Option<String>,
    /// Where a new instance starts.
    pub initial: String,
    /// smllm data.
    pub meta: MachineMeta,
    /// States (flat: nested `states` are not supported in v1).
    pub states: OrderedMap<StateNode>,
}

/// Machine `meta` (CFG-10).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct MachineMeta {
    /// smllm format version: 1.
    pub smllm: u32,
    /// Instance vocabulary (CFG-6).
    #[serde(default)]
    pub instance: Option<InstanceMeta>,
    /// Event types, declared once (CFG-7). Built-in names may set only `description`.
    #[serde(default)]
    pub events: Option<OrderedMap<EventMeta>>,
    /// Actions added to several states' `entry`/`exit` (CFG-11).
    #[serde(default)]
    pub shared_actions: Option<Vec<SharedActionSrc>>,
}

/// `meta.instance`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct InstanceMeta {
    /// What an instance is called in generated text (default `instance`).
    #[serde(default)]
    pub noun: Option<String>,
    /// The instance's external id.
    #[serde(default, rename = "ref")]
    pub r#ref: Option<RefMeta>,
}

/// `meta.instance.ref`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct RefMeta {
    /// The ref's param name (default `ref`), used by `enter` and `setRef`.
    #[serde(default)]
    pub param: Option<String>,
    /// Prompt for the ref.
    #[serde(default)]
    pub description: Option<String>,
    /// Pattern the ref must match.
    #[serde(default)]
    pub pattern: Option<String>,
}

/// `meta.events.<name>`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct EventMeta {
    /// Default guidance.
    #[serde(default)]
    pub description: Option<String>,
    /// Params: a JSON Schema object of string properties (CFG-7).
    #[serde(default)]
    pub params: Option<ParamsSchema>,
}

/// `type: object` JSON Schema subset.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct ParamsSchema {
    /// Always `object`.
    #[serde(rename = "type")]
    pub kind: String,
    /// Params.
    #[serde(default)]
    pub properties: OrderedMap<PropSchema>,
    /// Required param names.
    #[serde(default)]
    pub required: Vec<String>,
}

/// One param's schema.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PropSchema {
    /// Always `string` in v1 (CFG-16).
    #[serde(rename = "type")]
    pub kind: String,
    /// Prompt.
    #[serde(default)]
    pub description: Option<String>,
    /// Allowed values.
    #[serde(default, rename = "enum")]
    pub enum_values: Option<Vec<String>>,
    /// Pattern (ECMA-262 regex, as JSON Schema).
    #[serde(default)]
    pub pattern: Option<String>,
}

/// Before or after a state's own actions.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum PositionSrc {
    /// Before.
    Before,
    /// After.
    After,
}

/// One `meta.sharedActions` entry.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SharedActionSrc {
    /// States it applies to.
    pub states: Vec<String>,
    /// Before or after the state's own `entry`/`exit`.
    pub position: PositionSrc,
    /// Added to `entry`.
    #[serde(default)]
    pub entry: Option<Actions>,
    /// Added to `exit`.
    #[serde(default)]
    pub exit: Option<Actions>,
}

/// State `type`.
#[derive(Debug, Clone, Copy, PartialEq, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum StateType {
    /// The default.
    Atomic,
    /// Finishes the instance (CFG-12).
    Final,
}

/// One state.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StateNode {
    /// Description.
    #[serde(default)]
    pub description: Option<String>,
    /// `final` finishes the instance.
    #[serde(default, rename = "type")]
    pub kind: Option<StateType>,
    /// smllm data.
    #[serde(default)]
    pub meta: Option<StateMeta>,
    /// Entry actions; default prompt file `enter-<STATE>.md`.
    #[serde(default)]
    pub entry: Option<Actions>,
    /// Exit actions.
    #[serde(default)]
    pub exit: Option<Actions>,
    /// Event → transitions.
    #[serde(default)]
    pub on: Option<OrderedMap<Transitions>>,
    /// Eventless transitions: the state is left at once (DEC-2).
    #[serde(default)]
    pub always: Option<Transitions>,
}

/// State `meta` (CFG-10).
// @zen-impl: CFG-10_AC-1
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct StateMeta {
    /// May be entered directly from idle.
    #[serde(default)]
    pub entry_point: bool,
    /// Where `unmatched` goes (IDLE-2); at most one per machine.
    #[serde(default)]
    pub fallback: bool,
    /// Per-state param prompts: event → param → prompt (CFG-8).
    #[serde(default)]
    pub param_descriptions: Option<OrderedMap<OrderedMap<String>>>,
}

/// One transition or a guarded list (first match wins).
pub type Transitions = OneOrMany<StringOr<TransitionSrc>>;

/// A transition object.
#[derive(Debug, Clone, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct TransitionSrc {
    /// Target state; omit for a targetless (internal) transition.
    #[serde(default)]
    pub target: Option<String>,
    /// Guard.
    #[serde(default)]
    pub guard: Option<GuardSrc>,
    /// Transition actions.
    #[serde(default)]
    pub actions: Option<Actions>,
    /// Re-enter the state on a self transition (XState).
    #[serde(default)]
    pub reenter: Option<bool>,
    /// Per-state guidance for the event.
    #[serde(default)]
    pub description: Option<String>,
}

/// One action or a list.
pub type Actions = OneOrMany<StringOr<ActionSrc>>;

/// `{type, params}` actions (CFG-4).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(
    tag = "type",
    content = "params",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum ActionSrc {
    /// Text for the agent.
    Prompt(PromptParams),
    /// Run a command.
    Command(CommandParams),
    /// Set the instance's ref from the event's ref param.
    SetRef,
}

/// `prompt` params: exactly one of `text`, `file`.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct PromptParams {
    /// Inline text.
    #[serde(default)]
    pub text: Option<String>,
    /// A file, relative to the machine file, read at request time.
    #[serde(default)]
    pub file: Option<String>,
}

/// `command` params (DEC-4..7).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct CommandParams {
    /// A shell string, or an argv list (no shell).
    pub run: Run,
    /// Timeout, default 60.
    #[serde(default)]
    pub timeout_secs: Option<u64>,
    /// Working dir, relative to the machine file; default the session's.
    #[serde(default)]
    pub cwd: Option<String>,
}

/// `{type, params}` guards (CFG-5).
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(
    tag = "type",
    content = "params",
    rename_all = "camelCase",
    deny_unknown_fields
)]
pub enum GuardSrc {
    /// Exit 0 = true.
    Command(CommandParams),
    /// True when `state` has been entered at least `atLeast` times.
    Visits(VisitsParams),
}

/// `visits` params.
#[derive(Debug, Clone, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct VisitsParams {
    /// The state whose entries are counted.
    pub state: String,
    /// Threshold (counting the current entry).
    pub at_least: u32,
}
