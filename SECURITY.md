# Security Policy

## Supported versions

sllm is pre-1.0. Security fixes target the **latest release** and the `main`
branch only — older versions are not patched, so upgrade to the latest release
to pick up a fix.

## Reporting a vulnerability

Please report security issues **privately** through GitHub's private
vulnerability reporting:

1. Open the [Security tab](https://github.com/six5536/sllm/security) of the
   repository.
2. Click **Report a vulnerability** to start a private advisory.

Don't open a public issue for a suspected vulnerability. The maintainer will
acknowledge the report and coordinate a fix and a disclosure date with you.

## Scope

sllm is a skeleton: the shipped command only prints a greeting and touches no
files, so the surface that matters is the **supply chain** — the prebuilt
binaries and npm/crates.io packages the release pipeline publishes. In scope is
anything that lets a release ship something other than what the tagged source
builds: a tampered binary or archive, a `SHA256SUMS` that does not match, a
publish from outside the release workflow, or a dependency pulled in past the
`cargo-deny` and `cargo-audit` gates. Once real commands land here, extend this
section with the safety guarantees they make.
