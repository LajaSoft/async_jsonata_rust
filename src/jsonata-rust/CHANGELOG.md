# Changelog

All notable changes to this crate will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
### Added
- Stable product-facing API surface: `Parser`, `Expression`, `Evaluator`, `FunctionRegistry`, `Error`.
- Unified error type with JSONata code and structured context.
- Crate-level docs and runnable examples for parse/async/registry/error scenarios.
- Compatibility matrix and known deviations documentation.
- Golden, regression, and differential testing skeleton.
- Contributor/release/security governance docs.

### Changed
- README rewritten around crate contract, MSRV policy, SemVer policy, and roadmap.
- Cargo package metadata aligned for crates.io/docs.rs publication.

## [0.1.0] - 2026-02-08
### Added
- Initial parser and runtime building blocks.
