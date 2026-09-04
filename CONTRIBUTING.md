# Contributing to Rendrr

Thanks for considering a contribution! This project is small and intentionally
focused on one thing: rendering Handlebars-flavored DOCX templates with JSON
data, with optional in-process PDF output.

## Ground rules

- Keep the surface area small. New features should be configurable and opt-in;
  the default no-config experience should stay simple.
- No new required external services. The whole point of this project is that
  it ships as a single container with no sidecars.
- No new required auth modes. OAuth is the supported optional layer; if you
  want a different auth scheme, please open an issue first to discuss.

## Building from source

Most users should run Rendrr via Docker — see the
[README](README.md#run-it). Build from source only if you're hacking on the
codebase.

### System dependencies

`dxpdf` (Rendrr's PDF engine) links Skia, and `skia-bindings` drives its own
GN + ninja build, so the build host needs:

```bash
# Debian / Ubuntu
sudo apt-get install -y --no-install-recommends \
  clang cmake ninja-build python3 libfontconfig1-dev libfreetype-dev pkg-config

# macOS
brew install cmake ninja fontconfig freetype
```

You'll also need Rust 1.89+ (`rustup install stable`). The MSRV is pinned as
`rust-version` in `Cargo.toml`, and the CI job reads that value rather than
hardcoding one, so `Cargo.toml` is the only place to change it. It's set by the
dependency tree rather than by anything in this repo's own source, so bumping a
dependency can raise it.

### Local development loop

```bash
docker compose up -d                    # MinIO + bucket creation
cp .env.example .env                    # defaults already point at MinIO
cargo run                               # http://localhost:3000
```

The repo's `docker-compose.yml` is the contributor dev stack — it only brings
up the storage layer (MinIO), and you run Rendrr itself via `cargo run`. End
users get a different, self-contained compose recipe in the docs.

## Tests, lint, format

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```

CI runs all three on every PR and must pass before merge.

## Documentation

The docs site is an Astro app under `docs/`. To preview locally:

```bash
cd docs
npm install
npm run dev
```

Commits to `main` deploy to GitHub Pages via `.github/workflows/docs.yml`.

## Code coverage

```bash
cargo install cargo-llvm-cov
cargo llvm-cov --workspace --ignore-filename-regex 'main\.rs' --summary-only
cargo llvm-cov --workspace --ignore-filename-regex 'main\.rs' --html
# open target/llvm-cov/html/index.html
```

`main.rs` is excluded because it's just the binary entrypoint — config
loading, server binding, optional middleware wiring — which is exercised in
integration via the deployment process, not unit tests.

## Cutting a release

Releases are tag-driven. Pushing a `v*` tag (or publishing a GitHub Release,
which creates one) runs `.github/workflows/release.yml`: it re-runs fmt,
clippy, and the tests, then builds and pushes a multi-arch image to
`ghcr.io/portside-labs/rendrr`.

1. Bump `version` in `Cargo.toml`, then run any cargo command to refresh
   `Cargo.lock`.
2. Move the `Unreleased` section of `CHANGELOG.md` under the new version.
3. Commit, then tag with a leading `v`:

   ```bash
   git tag v0.2.0
   git push origin v0.2.0
   ```

The tag must match `Cargo.toml` exactly — the workflow fails fast if they
disagree, because the binary reports its version from `CARGO_PKG_VERSION` via
`--version` and `/health`, and a mismatch would ship an image whose tag and
self-reported version differ. For a release candidate, set the Cargo version
to `0.2.0-rc1` and tag `v0.2.0-rc1`; pre-release tags don't move `latest`.

`amd64` and `arm64` build on native runners in parallel and are merged into
one manifest list. Building `arm64` under QEMU would mean emulating dxpdf's
Skia compile, which is slow enough to risk the job timeout.

## Reporting issues

Please include:

- Rendrr version (`rendrr --version` or git SHA)
- A minimal reproduction (the DOCX template + the JSON payload, redacted)
- The exact error message and HTTP status code
- Whether OAuth and/or TLS were enabled
