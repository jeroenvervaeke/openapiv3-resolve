# Changelog

All notable changes to this project will be documented in this file.

## [0.2.0](https://github.com/jeroenvervaeke/openapiv3-resolve/compare/v0.1.1...v0.2.0) - 2026-09-15

### Features

- [**breaking**] resolve discriminator mappings into schema edges ([#9](https://github.com/jeroenvervaeke/openapiv3-resolve/pull/9))

## [0.1.1](https://github.com/jeroenvervaeke/openapiv3-resolve/compare/v0.1.0...v0.1.1) - 2026-09-11

### Features

- resolve a whole document into a reference-free tree ([#6](https://github.com/jeroenvervaeke/openapiv3-resolve/pull/6))

## [0.1.0](https://github.com/jeroenvervaeke/openapiv3-resolve/releases/tag/v0.1.0) - 2026-09-11

### Bug Fixes

- correct the hop bound and decode percent-escaped pointers

### CI

- release with release-plz and a git-cliff generated changelog
- run doctests, check docs, verify MSRV, drop chrome install

### Documentation

- record the 0.1.0 API in the changelog
- document public API and make the README example a doctest

### Features

- [**breaking**] typed errors, bounded ref walks, section-correct lookups

### Miscellaneous

- add contact email to package author
- add package author
- relicense under MIT and complete crates.io metadata

### Refactoring

- replace openapiv3 glob import with explicit imports

### Testing

- count allocations per thread so harness noise can't fail CI
