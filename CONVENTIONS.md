# Conventions

slipway follows the program's shared conventions: small typed structs, enums instead of `bool`
fields and parameters, typed errors (`thiserror`), no `unsafe`, short files, doc comments on every
public item, tests that never touch the real system. `ARCHITECTURE.md` here has the crate map and
the seams.

1. **Library first.** `slipway-core` is pure and usable on its own; `slipway` is a library with a
   thin `main.rs` and a thin `bin/slipway_apply.rs`. Every real I/O (the download, the privileged
   helper, the trust anchor, the clock, the login identity) sits behind a trait or a closure with a
   fake in the tests.
2. **Wire values stay at the edge.** `release.toml`'s text and the bus's strings and integers become
   enums and structs where they are read (`slipway-core/src/release/`, `service/object.rs`,
   `service/status.rs`); `slipway-core` never sees a `u32` that means something except through a
   named type (`UnixSeconds`, `Sha256Hex`, `BundlePath`).
3. **No panics on a bad request.** A malformed release, bundle or call is a refusal with a plain
   sentence, not a panic. `unwrap` and `expect` only in tests.
4. **Time, randomness and the environment are arguments.** `slipway-core` reads no clock, file or
   environment: the caller passes bytes, times and what it saw on disk. In `slipway`, only `main.rs`,
   `clock.rs`, `bootstrap.rs` and `login::current` read the real environment, boot id or clock; the
   rest take them as closures or fields (`Dirs::resolve`, `login::here`, `Layout`).
5. **Trust.** Nothing unverified is ever unpacked into place: the signature of `release.toml`, the
   bundle digest and every file's digest are checked by the privileged helper itself, whatever the
   daemon already checked. The helper takes its root and key from the system settings file, never
   from the caller. The release key is public; no secret is stored or logged.
6. **Plain language.** Nothing a person sees (a notification, the Software Update page, a command
   line answer) names a protocol, a bus, a bundle format or a tool; it says what happened and what
   to do (`problem.rs` is the one place those sentences live).
7. **Licence hygiene.** slipway is MIT OR Apache-2.0. Dependencies must pass `deny.toml`; no
   copyleft code is linked or copied. Other updaters are read for facts only (what a flow should
   do), never for code.
8. **`todo!()` lives on master only behind a frozen interface.** Each has a row in `FINDINGS.md`
   naming its fill lane and the test that proves it; that test is written and `#[ignore = "<lane>"]`d.
   A `todo!()` body binds its arguments (`let _ = (a, b);`), and private state a frozen body will read
   carries `#[expect(dead_code, reason = "<lane>")]`. (There are none today.)
9. **`#[non_exhaustive]`** on public enums that will grow; `Copy` only for heap-free values of at
   most two words.
10. **`cargo fmt -p <crate>`** to write; the gate runs `cargo fmt --all --check`.
11. **One test binary per crate** (`tests/it/main.rs`), tables over one-off cases.
