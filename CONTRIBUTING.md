# Contributing to MeshBBS

Thank you for your interest in contributing to MeshBBS! This document provides guidelines for contributing to the project.

## Code of Conduct

By participating in this project, you agree to maintain a respectful and inclusive environment for all contributors.

## Getting Started

### Prerequisites

- Rust 1.70 or higher
- Git
- A Meshtastic device for testing (optional but recommended)

### Development Setup

1. Fork the repository on GitHub
2. Clone your fork locally:
   ```bash
   git clone https://github.com/your-username/meshbbs.git
   cd meshbbs
   ```
3. Create a new branch for your feature:
   ```bash
   git checkout -b feature/your-feature-name
   ```
4. Build the project:
   ```bash
   cargo build
   ```
5. Run tests to ensure everything works:
   ```bash
   cargo test
   ```

## Making Changes

### Code Style

- Follow standard Rust formatting using `rustfmt`:
  ```bash
  cargo fmt
  ```
- Use `clippy` to catch common issues:
  ```bash
  cargo clippy
  ```
- Write clear, self-documenting code with appropriate comments
- Add documentation for public APIs

### Commit Guidelines

- Write clear, descriptive commit messages
- Use present tense ("Add feature" not "Added feature")
- Reference issues and pull requests when applicable
- Keep commits focused and atomic

### Testing

- Add tests for new functionality
- Ensure all existing tests pass
- Test with actual Meshtastic hardware when possible
- Document any new testing procedures

## Types of Contributions

### Bug Reports

When reporting bugs, please include:
- Clear description of the issue
- Steps to reproduce
- Expected vs actual behavior
- Environment details (OS, Rust version, hardware)
- Relevant log output

### Feature Requests

For new features:
- Describe the use case and motivation
- Provide detailed specifications when possible
- Consider backwards compatibility
- Discuss implementation approach

### Code Contributions

1. **Small fixes**: Direct pull requests are welcome
2. **New features**: Please open an issue first to discuss
3. **Breaking changes**: Require discussion and planning

## Pull Request Process

1. Update documentation as needed
2. Add or update tests for your changes
3. Ensure `cargo test` and `cargo clippy` pass
4. Update CHANGELOG.md with your changes
5. Submit pull request with clear description
6. Respond to review feedback promptly

### Pull Request Template

```markdown
## Description
Brief description of changes

## Related Issue
Fixes #(issue number)

## Type of Change
- [ ] Bug fix
- [ ] New feature
- [ ] Breaking change
- [ ] Documentation update

## Testing
- [ ] Tests pass locally
- [ ] New tests added (if applicable)
- [ ] Tested with hardware (if applicable)

## Checklist
- [ ] Code follows project style
- [ ] Self-review completed
- [ ] Documentation updated
- [ ] CHANGELOG.md updated
```

## Development Guidelines

### Architecture

- Follow the existing modular structure
- Keep modules focused and cohesive
- Use appropriate error handling with `anyhow`
- Prefer async/await for I/O operations

### Dependencies

- Avoid unnecessary dependencies
- Prefer well-maintained crates
- Document why specific dependencies are needed
- Consider optional features for heavy dependencies

### Documentation

- Document all public APIs
- Include examples in documentation
- Update README.md for user-facing changes
- Maintain inline code comments for complex logic

## Communication

- Use GitHub Issues for bug reports and feature requests
- Tag maintainers for urgent issues
- Be patient and respectful in all interactions
- Provide constructive feedback in reviews

## License

By contributing to MeshBBS, you agree that your contributions will be licensed under the Creative Commons Attribution-NonCommercial 4.0 International License.

## Recognition

Contributors will be recognized in:
- CHANGELOG.md for their contributions
- GitHub contributors list
- Project documentation (for significant contributions)

## Questions?

If you have questions about contributing, please:
1. Check existing issues and documentation
2. Open a new issue with the "question" label
3. Contact the maintainer: martinbogo@gmail.com

Thank you for contributing to MeshBBS! 🚀