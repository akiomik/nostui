# Contributing

Issues and pull requests are welcome.

Where code goes is written down already: [docs/architecture.md](docs/architecture.md)
has the layers, the direction dependencies may point, and the contracts that
connect them — what `domain` may not know, why `model` reports outcomes instead
of issuing effects, why `model::editor` is a deliberate exception.

What follows is the rest: the conventions nothing in `just lint` or CI checks.

## Language

Everything published is written in English — code, comments, documentation,
commit messages, and the titles and bodies of pull requests and issues. Where
something will be read decides it, not what kind of thing it is.

## Commit messages and pull request titles

[Conventional Commits][cc]: `type: summary`, with `feat`, `fix`, `docs`,
`refactor`, `perf`, `test`, `build`, `chore` and `revert` as the types. Scopes
are not used here; the only scoped commits are Dependabot's `build(deps)`.

The summary states what the change claims, not which file it touched:

```
fix: an empty composer published an empty note
test: "every profile" needs more than one profile
docs: a stopped media source is restarted by the next message, not never
```

Pull request titles take the same shape. Nothing verifies any of this — there
is no commit linter here, and no CI job reads a commit message.

## Tests

Every test lives in a `mod tests` at the end of the file it exercises. There is
no `tests/` directory today.

### A name says what is asserted

The name states the claim rather than the function under test —
`new_holds_no_profiles`, not `new` — and carries no `test_` prefix, since
`#[test]` already says that ([#541](https://github.com/akiomik/nostui/issues/541),
[#547](https://github.com/akiomik/nostui/pull/547)). Dropping the prefix is what
makes the first half load-bearing: `mod tests` pulls the outer module in with
`use super::*`, so a test left named after the function it calls shadows that
function.

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
- Return `Result` from the test instead of calling `unwrap`; use `expect` on an
  `Option`.

### Where the redraw directive is observable

Whether a `Command` was built `without_redraw` can only be read through
`tears::testing::TestStore::redraw_requested`, `Command::requests_redraw` being
crate-private to `tears`. So those tests live in `src/runtime.rs` even when the
decision they pin is made in `application::state`: construct a
`TestStore::<TearsApp<'static>>::new(flags)`, `send` the message, and read
`redraw_requested()`. `TestStore::new` asserts that no Tokio runtime is entered,
so the test must be a plain `#[test]` — `#[tokio::test]` panics.

## Before you push

[just](https://github.com/casey/just) runs what CI runs:

```console
$ just fmt
$ just lint
$ just test
```

CI runs these as separate jobs, so a formatting slip fails a run that clippy and
the tests passed locally — hand-edited grouped `use` imports are the usual
cause. `just lint` also needs [typos](https://github.com/crate-ci/typos) on
PATH.

One CI job is outside all three: `Docs` builds the documentation with warnings
denied. Run it by hand when the change touched a doc comment, since `just doc`
alone does not deny them:

```console
$ RUSTDOCFLAGS='-D warnings' just doc
```

Codecov comments on the pull request and is not a required check. The repository
has no `codecov.yml`, so the default `threshold: 0%` fails on any decrease at
all. When it goes red, read the uncovered lines rather than the percentage:

```console
$ cargo llvm-cov --all-features --workspace --show-missing-lines
```

Testable logic left uncovered is worth covering; the entry-point glue that takes
the terminal over or connects to relays cannot be unit-tested, and a red mark is
better than a contrivance that buys the percentage back. Say in the pull request
which of the two it was.

[cc]: https://www.conventionalcommits.org/en/v1.0.0/
