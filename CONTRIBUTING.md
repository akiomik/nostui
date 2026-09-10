# Contributing

Issues and pull requests are welcome.

Where code goes is written down already: [docs/architecture.md](docs/architecture.md)
has the layers, the direction dependencies may point, and the contracts that
connect them — what `domain` may not know, why `model` reports outcomes instead
of issuing effects, why `model::editor` is a deliberate exception.

What follows is the rest: the conventions that hold by agreement rather than by
a check, and the checks `just fmt`, `just lint` and `just test` leave out.

## Language

Everything published is written in English — code, comments, documentation,
commit messages, and the titles and bodies of pull requests and issues. Where
something will be read decides it, not what kind of thing it is.

## Commit messages and pull request titles

[Conventional Commits][cc]: `type: summary`, with `feat`, `fix`, `docs`,
`refactor`, `perf`, `test`, `build`, `chore` and `revert` as the types. A scope
is allowed and rarely taken — two commits in the history carry one, beside
Dependabot's `build(deps)`.

The summary states what the change claims, not which file it touched:

```
fix: an empty composer published an empty note
test: "every profile" needs more than one profile
docs: a stopped media source is restarted by the next message, not never
```

Pull request titles take the same shape. Nothing verifies any of this — there
is no commit linter here, and no CI job reads a commit message.

## Tests

Tests live in a `mod tests` at the end of the file they exercise — all of them
but the doctest on `NostrEvents::new`. There is no `tests/` directory today.

### A name says what is asserted

The name states the claim rather than the function under test —
`new_holds_no_profiles`, not `new` — and carries no `test_` prefix, since
`#[test]` already says that ([#541](https://github.com/akiomik/nostui/issues/541),
[#547](https://github.com/akiomik/nostui/pull/547)).

Dropping the prefix is what makes the first half more than taste where the
subject is a free function: `mod tests` pulls the outer module in with
`use super::*`, so a test named `shorten_npub` shadows the function it means to
call, and the call in its own body resolves to the test. An inherent method is
not exposed to that — `use super::*` does not put `UserState::new` in scope as a
bare `new` — so there the reason to name the assertion is just that the
assertion is what a reader needs.

A name that quantifies has to earn it. `every`, `only`, `in order` and `never`
each need enough cases that a wrong implementation fails: one profile does not
establish `clear_profiles_drops_every_profile`, and three sampled indices do not
establish `is_at_bottom_only_on_the_last_note`.

### A test nobody watched fail is not a test

The defect this repository keeps producing is not a wrong assertion but an inert
one — a test that passes whether or not the change under it is present. Break
the implementation, run `cargo test <filter>`, watch the failure, then put it
back. Reasoning that an assertion must be load-bearing has been wrong often
enough that review does not accept it.

Undo the mutation by editing it back out. `git checkout -- <file>` takes the
uncommitted work with it.

### Assertions

- Assert the whole value rather than a field at a time:
  `assert_eq!(foo, Foo { field: "bar" })` over a run of
  `assert_eq!(foo.field, "bar")`. A field-at-a-time test only fails on the
  fields it happened to name.
- Prefer the form that prints the value on failure: `assert_eq!(maybe, None)`
  over `assert!(maybe.is_none())`, `assert_eq!(xs, Vec::<T>::new())` over
  `assert_eq!(xs.len(), 0)`. Use plain `assert!` for a genuine `bool`.
- Do not test what a `derive` provides (`Debug`, `Default`, `Clone`,
  `PartialEq`).
- Return `Result` and use `?` rather than calling `unwrap`; use `expect` on an
  `Option`. This one is checked — `Cargo.toml` sets `unwrap_used = "warn"` and
  `just lint` denies warnings — so an `unwrap` fails the lint rather than
  review. Where `?` cannot reach, inside an `rstest` `#[case(...)]` argument for
  instance, the tree opens the module with `#![allow(clippy::unwrap_used)]`
  instead of contorting the case.

### Where the redraw directive is observable

Whether a `Command` was built `without_redraw` can only be read through
`tears::testing::TestStore::redraw_requested`, `Command::requests_redraw` being
crate-private to `tears`. So those tests live in `src/runtime.rs` even when the
decision they pin is made in `application::state`: construct a
`TestStore::<TearsApp<'static>>::new(flags)`, `send` the message, read
`redraw_requested()`, and end with `store.finish()`. `finish` is not bookkeeping:
the store checks that every message it produced was received, on drop if the test
did not ask, so skipping it fails the test somewhere other than its assertion.
`TestStore::new` also asserts that no Tokio runtime is entered, so the test must
be a plain `#[test]` — `#[tokio::test]` panics.

## Before you push

[just](https://github.com/casey/just) runs most of what CI runs:

```console
$ just fmt
$ just lint
$ just test
```

CI runs these as separate jobs, so a formatting slip fails a run that clippy and
the tests passed locally — hand-edited grouped `use` imports are the usual
cause. `just lint` also needs [typos](https://github.com/crate-ci/typos) on PATH,
which is what covers the `Spelling` job. Both jobs that run tests set
`TZ=Asia/Tokyo` and the justfile does not, so an assertion that reads local time
can pass here and fail there — `TextNoteWidget::formatted_created_at` renders in
the local zone, and its test pins the shape rather than a wall-clock value for
that reason.

Two CI jobs are outside all three. `Docs` builds the documentation with warnings
denied; run it by hand when the change touched a doc comment, since `just doc`
alone does not deny them:

```console
$ RUSTDOCFLAGS='-D warnings' just doc
```

`Code Coverage` is the other, and it is not a required check. The repository has
no `codecov.yml`, so the default `threshold: 0%` fails on any decrease at all.
When Codecov goes red, read the uncovered lines rather than the percentage:

```console
$ cargo llvm-cov --all-features --workspace --show-missing-lines
```

Testable logic left uncovered is worth covering; the entry-point glue that takes
the terminal over or connects to relays has no unit test today, and a red mark
there is better than a contrivance that buys the percentage back. Say in the
pull request which of the two it was.

[cc]: https://www.conventionalcommits.org/en/v1.0.0/
