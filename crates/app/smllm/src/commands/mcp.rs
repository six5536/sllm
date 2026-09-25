//! `smllm mcp`: the stdio MCP server with the one `smllm` tool (D3, HOST-5,
//! HOST-12, CLI-9).
// @zen-component: HOST-Mcp

use std::path::PathBuf;
use std::sync::Arc;

use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData, RoleServer, ServerHandler, ServiceExt as _};
use serde_json::{Map, Value, json};
use smllm_core::{AGENT_RULES, Bind};

use crate::error::{Error, Result};
use crate::output::EXIT_OK;
use crate::paths;
use crate::runtime::Runtime;

/// The tool's name.
pub const TOOL: &str = "smllm";

/// The tool description: the full agent rules (HOST-12).
// @zen-impl: HOST-12_AC-1
pub fn description() -> String {
    format!(
        "{AGENT_RULES}\n\nCall with {{ session }} alone to see where you are; with {{ session, event, \
         params }} to fire an event. params is an object of strings."
    )
}

/// The tool's input schema (CFG-16: params are strings).
pub fn input_schema() -> Map<String, Value> {
    let v = json!({
        "type": "object",
        "properties": {
            "session": { "type": "string", "description": "The session key from the latest <smllm> header, e.g. sm-k7f3q2." },
            "event": { "type": "string", "description": "The event to fire; omit to see where you are." },
            "params": {
                "type": "object",
                "description": "The event's params.",
                "additionalProperties": { "type": "string" }
            }
        },
        "required": ["session"]
    });
    match v {
        Value::Object(m) => m,
        _ => Map::new(),
    }
}

/// Serve one tool call: `(ok, text)`.
// @zen-impl: CFG-16_AC-1
pub fn call(
    args: &Map<String, Value>,
    cwd: &std::path::Path,
    explicit: Option<&std::path::Path>,
) -> (bool, String) {
    match call_inner(args, cwd, explicit) {
        Ok(r) => r,
        Err(e) => (false, format!("error: {e}")),
    }
}

fn call_inner(
    args: &Map<String, Value>,
    cwd: &std::path::Path,
    explicit: Option<&std::path::Path>,
) -> Result<(bool, String)> {
    let str_arg = |k: &str| match args.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) if s.is_empty() => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(Error::msg(format!("{k} must be a string"))),
    };
    let session = str_arg("session")?;
    let event = str_arg("event")?;
    let mut params = Vec::new();
    match args.get("params") {
        None | Some(Value::Null) => {}
        Some(Value::Object(m)) => {
            for (k, v) in m {
                match v {
                    Value::String(s) => params.push((k.clone(), s.clone())),
                    _ => return Err(Error::msg(format!("param {k} must be a string"))),
                }
            }
        }
        Some(_) => return Err(Error::msg("params must be an object of strings")),
    }
    let mut rt = match &session {
        Some(k) => Runtime::for_session(k)?,
        None => Runtime::lookup(explicit, cwd)?,
    };
    let reply = match &event {
        None => {
            let key = session.ok_or(smllm_core::Error::MissingSession)?;
            rt.with(|e, h| e.view(h, &key))?
        }
        Some(ev) => {
            let cwd_s = cwd.display().to_string();
            let configs = rt.configs.clone();
            let bind = Bind {
                harness: "mcp",
                host_session: None,
                cwd: &cwd_s,
                configs: &configs,
            };
            rt.with(|e, h| e.fire(h, session.as_deref(), ev, &params, &bind))?
        }
    };
    Ok((reply.ok, reply.text))
}

#[derive(Clone)]
struct Server {
    cwd: Arc<PathBuf>,
    explicit: Option<Arc<PathBuf>>,
}

impl ServerHandler for Server {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
            .with_server_info(Implementation::new("smllm", env!("CARGO_PKG_VERSION")))
            .with_instructions(AGENT_RULES)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<ListToolsResult, ErrorData> {
        Ok(ListToolsResult {
            tools: vec![Tool::new(TOOL, description(), Arc::new(input_schema()))],
            ..Default::default()
        })
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> std::result::Result<CallToolResponse, ErrorData> {
        if request.name != TOOL {
            return Err(ErrorData::invalid_params(
                format!("no tool {}", request.name),
                None,
            ));
        }
        let args = request.arguments.unwrap_or_default();
        let cwd = self.cwd.clone();
        let explicit = self.explicit.clone();
        // Guards and actions may run commands for minutes: off the runtime.
        let (ok, text) = tokio::task::spawn_blocking(move || {
            call(&args, &cwd, explicit.as_deref().map(|p| p.as_path()))
        })
        .await
        .unwrap_or_else(|e| (false, format!("error: {e}")));
        let content = vec![ContentBlock::text(text)];
        Ok(if ok {
            CallToolResult::success(content)
        } else {
            CallToolResult::error(content)
        }
        .into())
    }
}

/// `smllm mcp`.
// @zen-impl: CLI-9_AC-1
pub fn serve(explicit: Option<&std::path::Path>) -> Result<u8> {
    let cwd = std::env::current_dir().map_err(|e| Error::io(std::path::Path::new("."), e))?;
    let _ = paths::user_state_dir()?;
    let server = Server {
        cwd: Arc::new(cwd),
        explicit: explicit.map(|p| Arc::new(p.to_path_buf())),
    };
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| Error::msg(format!("tokio: {e}")))?;
    rt.block_on(async move {
        let running = server
            .serve(rmcp::transport::stdio())
            .await
            .map_err(|e| Error::msg(format!("mcp: {e}")))?;
        running
            .waiting()
            .await
            .map_err(|e| Error::msg(format!("mcp: {e}")))?;
        Ok::<(), Error>(())
    })?;
    Ok(EXIT_OK)
}
