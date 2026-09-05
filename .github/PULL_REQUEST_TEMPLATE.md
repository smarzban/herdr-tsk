## Summary

## Test plan

- [ ] `cargo fmt --check`
- [ ] `cargo clippy --all-targets -- -D warnings`
- [ ] `cargo test`
- [ ] Manual herdr open-board smoke (if host paths changed)
- [ ] `cd site && npm ci && npm test && npm run build` (if `site/` changed)
