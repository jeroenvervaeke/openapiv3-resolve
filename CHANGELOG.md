# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0]

Initial release.

- `Resolve` on `OpenAPI`: `resolve_ref::<T>(pointer)` resolves a full pointer
  such as `#/components/schemas/Pet`, following chains of `$ref`s.
- `ResolveWithOpenAPI<T>` on `ReferenceOr<T>` and `ReferenceOr<Box<T>>`.
- `ResolveOptionalWithOpenAPI<T>` on `Option<R>`: an absent field is `Ok(None)`,
  not an error.
- `ResolveError` reports why a reference failed — a dangling name, a reference
  into the wrong section, a pointer into another document, or a chain that
  never terminates — instead of a bare `None`.
- Resolvable targets are the nine `#/components` sections and `#/paths`, listed
  by `Section`. Pointers are percent-decoded as URI fragments and then RFC 6901
  unescaped, so `#/paths/~1pets~1%7Bid%7D` names the path `/pets/{id}`.
- `Component` maps a Rust type to its section; it is sealed so the two cannot
  disagree.
- Reference chains are walked iteratively and capped at `MAX_REFERENCE_HOPS`,
  so a cyclic document errors instead of overflowing the stack.

[Unreleased]: https://github.com/jeroenvervaeke/openapiv3-resolve/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jeroenvervaeke/openapiv3-resolve/releases/tag/v0.1.0
