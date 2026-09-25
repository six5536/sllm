//! smllm-core: the engine that puts declarative state machines in charge of
//! an LLM agent's turn loop.
//!
//! `no_std` + `alloc`, with no IO, time or randomness: a host supplies them
//! through the traits in [`host`] (NFR-1). The model in [`model`] is built by
//! `smllm-format` from YAML, or deserialised from `smllm compile` JSON (the
//! `serde` feature).
//!
//! ```
//! use smllm_core::{Bind, Engine, host::MemoryStore, model::Config};
//! # use smllm_core::host::*;
//! # struct H; impl Guard for H { fn supports(&self,_:&str)->bool{false} fn check(&mut self,_:&Call<'_>)->Outcome{Outcome{ok:false,detail:String::new()}} }
//! # impl Action for H { fn supports(&self,_:&str)->bool{false} fn run(&mut self,_:&Call<'_>)->Outcome{Outcome{ok:false,detail:String::new()}} }
//! # impl InstructionSource for H { fn read(&self,_:&str)->Result<Option<String>,String>{Ok(None)} }
//! # impl Matcher for H { fn is_match(&self,_:&str,_:&str)->Result<bool,String>{Ok(true)} }
//! # impl Clock for H { fn now_ms(&self)->u64{0} }
//! # impl Ids for H { fn random(&mut self)->u64{42} }
//! # let (mut g, mut a, mut i, h) = (H, H, H, H);
//! let mut store = MemoryStore::default();
//! let mut host = Host { store: &mut store, guards: &mut g, actions: &mut a, source: &h, matcher: &h, clock: &h, ids: &mut i };
//! let engine = Engine::new(Config::default());
//! let reply = engine.bind(&mut host, &Bind { harness: "none", ..Bind::default() })?;
//! assert!(reply.text.starts_with("<smllm>\nsession sm-"));
//! # Ok::<(), smllm_core::Error>(())
//! ```

#![no_std]
#![warn(missing_docs)]

extern crate alloc;

mod engine;
pub mod host;
pub mod model;
mod prelude;
pub mod record;
mod render;
mod utils;

pub use engine::{BUILTINS, Bind, Engine, Error, Location, Offer, ParamView, Reply, Stop};
pub use render::{AGENT_RULES, format_utc};
pub use utils::SmallMap;
