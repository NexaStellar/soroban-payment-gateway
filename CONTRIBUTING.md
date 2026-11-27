# Contributing

Thanks for contributing to the Soroban Payment Gateway. This is an unaudited
contract prototype; do not deploy it with real funds.

## Development setup

Install a current stable Rust toolchain, `rustfmt`, `clippy`, and the
`wasm32-unknown-unknown` target for Wasm builds. Then run:

```bash
make check
```

`make check` runs formatting checks, Clippy, and workspace tests. The local
tests do not require a funded account or live network access.

## Pull requests

- Explain the behavior change and link related issues.
- Add tests for success, authorization, failure, and storage behavior where
  relevant.
- Keep contract entry points and emitted events documented in `README.md`.
- Run `make check` before requesting review.
- Do not include credentials, private keys, or funded account data.

Contract changes can affect custody and authorization. Explain those effects in
the pull request and prefer local Soroban tests over network deployments.

## Drips Wave

Only work on issues that are currently listed in the relevant Wave Program and
follow that program's application/assignment process. Repository approval and
issue eligibility are controlled by the program organizers; local labels or
draft backlog entries do not grant eligibility.

## Security reports

Use [GitHub private vulnerability reporting](https://github.com/NexaStellar/soroban-payment-gateway/security/advisories/new)
for suspected vulnerabilities. Do not post exploit details publicly.

See the [Code of Conduct](CODE_OF_CONDUCT.md). Contributions are licensed under
the repository's MIT License.
