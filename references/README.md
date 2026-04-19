# References

## community-solid-server

This is the JavaScript Solid Server (JSS) by the Community Solid
Server team, vendored at this path for read-only API parity reference
while solid-pod-rs ports features to Rust.

The directory is a symlink to the repository's top-level
`JavaScriptSolidServer/` directory, which carries the JSS source
tree. Upstream:

    https://github.com/CommunitySolidServer/CommunitySolidServer

## Usage rules

- **READ ONLY.** This tree is not built, not executed, not linked,
  not watched. It is source-of-truth documentation for Solid
  Protocol behaviour.
- **Do not modify files under this tree.** Upstream-style edits go
  into `crates/solid-pod-rs/` as Rust ports.
- **License:** JSS is MIT licensed. See
  `community-solid-server/LICENSE.md` (original upstream file).

## Parity tracking

`crates/solid-pod-rs/PARITY-CHECKLIST.md` catalogues every JSS feature
and its status in solid-pod-rs (present / partial / missing) along
with the target phase for each item.

## Adding as a git submodule (future)

In a network-available CI environment the vendoring will be replaced
with a submodule:

```bash
git submodule add https://github.com/CommunitySolidServer/CommunitySolidServer.git \
    references/community-solid-server
```

For the Phase 1 sprint the plain-directory vendor is used because
submodule fetch is not available in the sandboxed build environment.
