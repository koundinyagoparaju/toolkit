<!-- Every tool and chain on the live site got there through a reviewed PR —
     that's the security model, not just process. Thanks for contributing. -->

**What & why**:

**Checklist**
- [ ] `cargo test` passes
- [ ] `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` are clean
- [ ] If a new tool: it's pure (no network / filesystem / clock / ambient randomness; randomness via an entropy port)
- [ ] If a new dependency: pure-Rust, `default-features = false` where possible, and justified above
- [ ] If the web app changed: `npm run build` succeeds
- [ ] Docs/CHANGELOG updated if user-facing
