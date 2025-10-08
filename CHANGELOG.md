# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2024-01-XX

### Added
- Initial release of global_cell
- Thread-safe global cell implementation
- Async/await support with Tokio
- Read and write locks (async and blocking)
- Functional access patterns (`with`, `with_mut`, etc.)
- Optional `serialize` feature for Serde support
- Optional `watch` feature for change notifications
- Comprehensive test suite
- Documentation and examples

### Fixed
- Edition specification in Cargo.toml (2024 → 2021)
- Removed unused `lazy_static` dependency
- Fixed test isolation issues
- Applied rustfmt formatting

### Infrastructure
- Added README.md with usage examples
- Added CI/CD workflow with GitHub Actions
- Added CHANGELOG.md
- Enhanced Cargo.toml metadata
