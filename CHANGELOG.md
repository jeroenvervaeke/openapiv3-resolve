# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0]

Initial release.

- `Resolve<T>` on `OpenAPI`, resolving pointers like `#/components/schemas/Pet`.
- `ResolveWithOpenAPI<T>` on `ReferenceOr<T>`, `ReferenceOr<Box<T>>` and their
  `Option<...>` variants.
- `ResolveWithOpenAPIAndPath<T>` on `Components` and
  `IndexMap<String, ReferenceOr<T>>`.

[Unreleased]: https://github.com/jeroenvervaeke/openapiv3-resolve/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/jeroenvervaeke/openapiv3-resolve/releases/tag/v0.1.0
