//! Integration tests: exercise `sllm-core` through its public API only, the
//! way the binary (and any other consumer) sees it.

use sllm_core::{DEFAULT_NAME, Error, greet};

#[test]
fn the_crate_root_re_exports_what_a_consumer_needs() {
    // Reached via `sllm_core::greet`, not `sllm_core::greeting::greet`: the
    // re-exports are part of the API surface, so a test pins them.
    let g = greet(Some("ada")).unwrap();
    assert_eq!(g.name, "ada");
    assert_eq!(g.message, "Hello, ada!");
}

#[test]
fn the_default_name_is_used_when_none_is_given() {
    assert_eq!(greet(None).unwrap().name, DEFAULT_NAME);
}

#[test]
fn errors_carry_a_message_the_cli_can_print() {
    let e = greet(Some("")).unwrap_err();
    assert!(matches!(e, Error::Config(_)));
    assert_eq!(e.to_string(), "config error: name must not be blank");
}
