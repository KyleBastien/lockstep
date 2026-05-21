# Changelog

All notable changes to this project will be documented in this file.

This project follows [Conventional Commits](https://www.conventionalcommits.org/);
release-please uses commit messages on `main` to generate this file and bump
versions automatically.

## [0.1.14](https://github.com/KyleBastien/lockstep/compare/v0.1.13...v0.1.14) (2026-05-21)


### Features

* **compare:** helper_call_site + destructure_then_narrow + catch fallback shapes ([#30](https://github.com/KyleBastien/lockstep/issues/30)) ([6ebbaee](https://github.com/KyleBastien/lockstep/commit/6ebbaee7217f90c8168bc89125fc5b47864057c4))

## [0.1.13](https://github.com/KyleBastien/lockstep/compare/v0.1.12...v0.1.13) (2026-05-21)


### Features

* **compare:** refinements to v0.1.12 narrowing rules ([#28](https://github.com/KyleBastien/lockstep/issues/28)) ([3e86a62](https://github.com/KyleBastien/lockstep/commit/3e86a6251d342981f506259bfdf5bc989be5bca3))

## [0.1.12](https://github.com/KyleBastien/lockstep/compare/v0.1.11...v0.1.12) (2026-05-20)


### Features

* **compare:** unknown_catch_narrowing + promise_settled_discrimination + pure_narrowing_helper ([#26](https://github.com/KyleBastien/lockstep/issues/26)) ([80d30ad](https://github.com/KyleBastien/lockstep/commit/80d30ad3d217f89b9b07c7e73a4f88cea9bd7edc))

## [0.1.11](https://github.com/KyleBastien/lockstep/compare/v0.1.10...v0.1.11) (2026-05-20)


### Features

* **compare:** allow_dead_defensive_optional_chain_removal ([#24](https://github.com/KyleBastien/lockstep/issues/24)) ([50413eb](https://github.com/KyleBastien/lockstep/commit/50413ebec746853c7e314ec196079bdefdb62e07))

## [0.1.10](https://github.com/KyleBastien/lockstep/compare/v0.1.9...v0.1.10) (2026-05-20)


### Features

* **compare:** non_null_alias_local + defensive_log_guard + guard composition ([#22](https://github.com/KyleBastien/lockstep/issues/22)) ([76a9fff](https://github.com/KyleBastien/lockstep/commit/76a9fff8662509e50029f64ef1ed3d1f0209e7ac))

## [0.1.9](https://github.com/KyleBastien/lockstep/compare/v0.1.8...v0.1.9) (2026-05-20)


### Features

* **compare:** five strict-TS normalization equivalence rules ([#20](https://github.com/KyleBastien/lockstep/issues/20)) ([4e29e14](https://github.com/KyleBastien/lockstep/commit/4e29e14542bcdf73f1a9d9398b934b3b05e7dbc6))

## [0.1.8](https://github.com/KyleBastien/lockstep/compare/v0.1.7...v0.1.8) (2026-05-19)


### Features

* **compare:** allow_nullish_widening equivalence rule ([#18](https://github.com/KyleBastien/lockstep/issues/18)) ([b4413e9](https://github.com/KyleBastien/lockstep/commit/b4413e9897b5dc6e02c7107f0126c6bea74493fc))

## [0.1.7](https://github.com/KyleBastien/lockstep/compare/v0.1.6...v0.1.7) (2026-05-19)


### Features

* **compare:** enforce optional-chain defensiveness direction ([#15](https://github.com/KyleBastien/lockstep/issues/15)) ([2cc9d86](https://github.com/KyleBastien/lockstep/commit/2cc9d867778a6f4b187b8c72d7c1eacf00d6cde0))
* **compare:** recursive AST match for cache-alias values ([#17](https://github.com/KyleBastien/lockstep/issues/17)) ([d42217b](https://github.com/KyleBastien/lockstep/commit/d42217b47f6d3665c1d5d6122d3749250f8d8f19))

## [0.1.6](https://github.com/KyleBastien/lockstep/compare/v0.1.5...v0.1.6) (2026-05-19)


### Bug Fixes

* **compare:** recognize constructor-assigned head caches ([#13](https://github.com/KyleBastien/lockstep/issues/13)) ([e73c45b](https://github.com/KyleBastien/lockstep/commit/e73c45b8b6ef591fee05c7280fa5bee0affadbb8))

## [0.1.5](https://github.com/KyleBastien/lockstep/compare/v0.1.4...v0.1.5) (2026-05-19)


### Bug Fixes

* **strip:** preserve binding when stripping optional parameter ([#11](https://github.com/KyleBastien/lockstep/issues/11)) ([4b6a1df](https://github.com/KyleBastien/lockstep/commit/4b6a1dfc7b28cdd69e455460e521fb9a384d8e28))

## [0.1.4](https://github.com/KyleBastien/lockstep/compare/v0.1.3...v0.1.4) (2026-05-19)


### Features

* **compare:** allow_array_first_element_or_null equivalence ([#9](https://github.com/KyleBastien/lockstep/issues/9)) ([7d0d8c9](https://github.com/KyleBastien/lockstep/commit/7d0d8c9eed4fe1b65b836c74844b5fc39843d6a2))

## [0.1.3](https://github.com/KyleBastien/lockstep/compare/v0.1.2...v0.1.3) (2026-05-19)


### Bug Fixes

* **ci:** require conventional PR titles ([#7](https://github.com/KyleBastien/lockstep/issues/7)) ([dac1b4f](https://github.com/KyleBastien/lockstep/commit/dac1b4f9d0a3d03108bc331185630bd1fdfdac56))

## [0.1.2](https://github.com/KyleBastien/lockstep/compare/v0.1.1...v0.1.2) (2026-05-19)


### Features

* **pairing:** match [@ts-nocheck](https://github.com/ts-nocheck) alongside [@ts-ignore](https://github.com/ts-ignore) on base ([#5](https://github.com/KyleBastien/lockstep/issues/5)) ([044a646](https://github.com/KyleBastien/lockstep/commit/044a646dbde1d45e440c9eaade9f00a5b8398616))
* **plugin:** add Claude Code marketplace.json manifest ([#3](https://github.com/KyleBastien/lockstep/issues/3)) ([6957c8c](https://github.com/KyleBastien/lockstep/commit/6957c8cbc69fe2dda21cfc5e6e0457b9f008215a))

## [0.1.1](https://github.com/KyleBastien/lockstep/compare/v0.1.0...v0.1.1) (2026-05-18)


### Features

* initial lockstep release ([66b42d5](https://github.com/KyleBastien/lockstep/commit/66b42d569bf1a62cbe084cc634d8cea2ccc51e49))


### Bug Fixes

* **ci:** unblock clippy and Windows linker ([#1](https://github.com/KyleBastien/lockstep/issues/1)) ([9b00908](https://github.com/KyleBastien/lockstep/commit/9b00908ec79cd03eb25d674fa24168b7e121a0fd))

## 0.1.0

Initial release.
