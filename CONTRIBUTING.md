# Contributing

Issues and pull requests are welcome.

Where code goes is written down already: [docs/architecture.md](docs/architecture.md)
has the layers, the direction dependencies may point, and the contracts that
connect them.

What follows is the rest: the conventions that hold by agreement rather than by
a check, and the checks `just fmt`, `just lint` and `just test` leave out.

## Language

Everything published is written in English — code, comments, documentation,
commit messages, and the titles and bodies of pull requests and issues. Where
something will be read decides it, not what kind of thing it is.

That is about prose somebody wrote to be read. Text that a test or a benchmark
operates on is data: `wrap_text`'s CJK literals are the double-width columns its
tests and its bench are about, and replacing them with ASCII would leave the
tests green and the benchmark measuring something else.

## Commit messages and pull request titles

[Conventional Commits][cc]: `type: summary`, with `feat`, `fix`, `docs`,
`refactor`, `perf`, `test`, `build`, `ci`, `chore` and `revert` as the types, an
optional scope, and `!` before the `:` for a breaking change.

The summary states what the change claims, not which file it touched:

```
fix: an empty composer published an empty note
test: "every profile" needs more than one profile
docs: a stopped media source is restarted by the next message, not never
```

Pull request titles take the same shape. Nothing verifies any of this.

## Tests

Tests live in a `mod tests` at the end of the file they exercise, the one
doctest aside. `tests/` holds the exception: a test that cannot be written
against the crate from the inside, because what it needs to observe is only
reachable through the public API or only produced by something outside the
process.

There is one today. `tests/publish_verdict.rs` stands up a local relay and
publishes to it, because `EventSendStatus::Ack` wraps a type nostr-sdk gives no
public constructor — a relay answering `OK true` is the only way to obtain one,
so the accepting half of the publish verdict has no unit test to be written
([#523](https://github.com/akiomik/nostui/issues/523)). Reach for `tests/` when
that is the situation, not because a test feels like an integration test.

### A name says what is asserted

The name states the claim rather than the function under test —
`new_holds_no_profiles`, not `new` — and carries no `test_` prefix, since
`#[test]` already says that
([#541](https://github.com/akiomik/nostui/issues/541),
[#547](https://github.com/akiomik/nostui/pull/547)). Where the subject is a free
function the prefix was load-bearing as well: `mod tests` pulls the outer module
in with `use super::*`, so a test named `shorten_npub` shadows the function it
means to call.

A name that quantifies has to earn it. `every`, `only`, `in order` and `never`
each need enough cases in the body that a wrong implementation fails.

### A test nobody watched fail is not a test

The defect this repository keeps producing is not a wrong assertion but an inert
one — a test that passes whether or not the change under it is present. Break
the implementation, run `cargo test <filter>`, watch the failure, then put it
back. Reasoning that an assertion must be load-bearing has been wrong often
enough that review does not accept it.

Undo the mutation by editing it back out. `git checkout -- <file>` takes the
uncommitted work with it.

### Assertions

- Assert the whole value rather than a field at a time. A field-at-a-time test
  only fails on the fields it happened to name.
- Prefer the form that prints the value on failure: `assert_eq!(maybe, None)`
  over `assert!(maybe.is_none())`. Use plain `assert!` for a genuine `bool`.
- Do not test what a `derive` provides.
- Return `Result` and use `?` rather than calling `unwrap`. This one is checked:
  `Cargo.toml` warns on `unwrap_used` and `just lint` denies warnings. Where `?`
  cannot reach, put `#[allow(clippy::unwrap_used)]` on the item that needs it
  rather than on the module, which would cover every test added later too.

### Where the redraw directive is observable

Whether a `Command` was built `without_redraw` can only be read through
`tears::testing::TestStore::redraw_requested`, so those tests live in
`src/runtime.rs` even when the decision they pin is made in `application::state`.
Three things about the store decide whether such a test works:

- `new` asserts that no Tokio runtime is entered, so the test is a plain
  `#[test]`; `#[tokio::test]` panics.
- Read the flag immediately after the `send` under test. It holds one command's
  directive, so anything else dispatched is what you read instead.
- End with `finish()`. Skipping it does not skip the check it makes: the same
  one runs at drop, and fails there rather than at the assertion.

## Before you push

[just](https://github.com/casey/just) runs most of what CI runs:

```console
$ just fmt
$ just lint
$ just test
```

`just fmt` rewrites rather than reports, so commit what it changed: CI runs
`cargo fmt --all --check`, where a reformat left uncommitted fails. `just lint`
needs [typos](https://github.com/crate-ci/typos) on PATH. Both CI jobs that run
tests set `TZ=Asia/Tokyo` and the justfile does not, so an assertion that reads
local time can pass here and fail there.

Two CI jobs are outside all three. `Docs` builds the documentation with warnings
denied, which `just doc` alone does not:

```console
$ RUSTDOCFLAGS='-D warnings' just doc
```

`Code Coverage` is the other, and it is not a required check. When Codecov goes
red, read the uncovered lines rather than the percentage — `just test-cov` gives
the summary, and the lines take a flag it does not pass:

```console
$ cargo llvm-cov --all-features --workspace --show-missing-lines
```

Both need [cargo-llvm-cov](https://github.com/taiki-e/cargo-llvm-cov), which CI
installs for itself and a checkout does not have.

Testable logic left uncovered is worth covering; the entry-point glue that takes
the terminal over or connects to relays has no unit test today, and a red mark
there is better than a contrivance that buys the percentage back. Say in the
pull request which of the two it was.

[cc]: https://www.conventionalcommits.org/en/v1.0.0/
