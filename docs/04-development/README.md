# Development Documentation

This directory contains development guides, best practices, and roadmaps.

## Development Guides

- **[Next Steps](next-steps.md)** - Production roadmap and timeline
- **[Best Practices](best-practices.md)** - Development standards and guidelines
- **[Command Reference](command-reference.md)** - Common development commands

## Development Process

- **[Clone Remediation Plan](clone-remediation-plan.md)** - Code quality improvement plan

## Getting Started with Development

1. Review **[Next Steps](next-steps.md)** for current priorities
2. Follow **[Best Practices](best-practices.md)** for code quality
3. Use **[Command Reference](command-reference.md)** for common tasks

## Contribution Guidelines

- All services must compile without warnings
- Use fixed-point arithmetic for financial calculations
- Follow existing patterns for gRPC service implementation
- Comprehensive error handling with proper Result types
- Performance-critical paths must be allocation-free

## Priority Areas

1. **Highest Priority**: Complete service executables (missing main.rs files)
2. **High Priority**: Service integration and end-to-end testing
3. **Medium Priority**: Deployment infrastructure and monitoring
4. **Lower Priority**: Advanced features and optimizations