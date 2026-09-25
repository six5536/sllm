# Security Policy

## Supported versions

smllm is pre-1.0. Security fixes target the **latest release** and the `main`
branch only — older versions are not patched, so upgrade to the latest release
to pick up a fix.

## Reporting a vulnerability

Please report security issues **privately** through GitHub's private
vulnerability reporting:

1. Open the [Security tab](https://github.com/six5536/smllm/security) of the
   repository.
2. Click **Report a vulnerability** to start a private advisory.

Don't open a public issue for a suspected vulnerability. The maintainer will
acknowledge the report and coordinate a fix and a disclosure date with you.

## Scope

State machine files are trusted like a Makefile: their `command` guards and actions run shell
commands with your permissions (NFR-5). Treat a `.smllm/` directory from an untrusted
repository as you would its build scripts. Text from the LLM reaches commands only as
environment variables (`SMLLM_PARAM_*`, `SMLLM_REF`, …) and is never interpolated into a
command line. In scope:

- An event param, or any other agent input, that can reach a command other than as an
  environment variable.
- Writes outside the paths smllm documents: `.smllm/`, the harness files it installs, and
  `~/.local/state/smllm`.
- Supply-chain problems in what the release pipeline publishes: tampered binaries or archives,
  mismatched `SHA256SUMS`, a publish from outside the release workflow, or a dependency that
  got past the `cargo-deny` and `cargo-audit` gates.
