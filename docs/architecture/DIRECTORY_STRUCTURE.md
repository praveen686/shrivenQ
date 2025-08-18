# ShrivenQuant Directory Structure

## Root Directory Organization

```
ShrivenQuant/
├── services/          # ✅ Microservices (20 services)
├── proto/            # ✅ Protocol buffer definitions
├── scripts/          # ✅ Operational scripts
├── docs/             # ✅ Documentation
├── reports/          # ✅ Generated reports
├── tests/            # ⚠️ Integration tests (minimal)
├── tools/            # ✅ Development tools
├── config/           # ✅ Configuration files
├── target/           # (Generated) Build artifacts
├── Cargo.toml        # Workspace configuration
├── Cargo.lock        # Dependency lock file
└── README.md         # Project overview
```

## Directory Purposes

### `/services` - Core Microservices ✅
All business logic organized as microservices:
- 17 implemented services
- 3 stub services
- Each service is self-contained
- gRPC communication between services

### `/proto` - Protocol Definitions ✅
gRPC protocol buffer definitions:
- Service interfaces
- Message types
- Shared data structures

### `/scripts` - Operational Scripts ✅
Utility scripts for operations:
- `deployment/` - Deployment scripts
- `monitoring/` - Monitoring utilities
- `testing/` - Test scripts
- `utils/` - General utilities

### `/docs` - Documentation ✅
Technical documentation:
- Architecture documents
- Getting started guides
- Development roadmap
- Service documentation

### `/reports` - Generated Output ✅
System-generated reports:
- `benchmark/` - Performance reports
- `compliance/` - Code compliance reports

### `/tests` - Integration Tests ⚠️
Test suite (currently minimal):
- 1 stub integration test
- Needs significant expansion

### `/tools` - Development Tools ✅
Standalone development utilities:
- `sq-compliance/` - Code compliance checker
- `sq-remediator/` - Code fix tool

### `/config` - Configuration ✅
Configuration files:
- Service configs
- Environment settings
- Secrets (encrypted)

## What Was Removed

### Removed Directories (Not Aligned)
- `/ui` - Empty UI structure (should be a service)
- `/ml` - Empty ML structure (now ml-inference service)
- `/python` - Legacy Python code (use Rust services)
- `/deployment` - Empty deployment structure
- `/infrastructure` - Empty infrastructure docs
- `/logs` - Local log files (use logging service)

### Removed from `/tests`
- `/python_legacy` - Old Python tests

### Removed from `/docs`
- Hyperbolic architecture reports
- Redundant status documents
- Outdated notes

## Architectural Principles

1. **Everything is a Service**
   - All functionality in `/services`
   - No standalone applications
   - Consistent service structure

2. **Clean Root Directory**
   - Only essential directories at root
   - No temporary or generated files
   - Clear purpose for each directory

3. **Separation of Concerns**
   - Code in `/services`
   - Config in `/config`
   - Scripts in `/scripts`
   - Docs in `/docs`

4. **No Redundancy**
   - Single source of truth
   - No duplicate implementations
   - Clear ownership

## Adding New Components

### To Add a New Service
```bash
# Create in services directory
mkdir -p services/new-service/src
# Add to Cargo.toml workspace
# Follow existing service patterns
```

### To Add Documentation
```bash
# Add to appropriate docs subdirectory
docs/architecture/  # Architecture docs
docs/development/   # Development guides
```

### To Add Scripts
```bash
scripts/deployment/  # Deployment scripts
scripts/testing/     # Test scripts
scripts/utils/      # Utilities
```

## What NOT to Add to Root

❌ Language-specific directories (e.g., `/python`, `/java`)
❌ UI/Frontend code (should be a service)
❌ Temporary files or logs
❌ Build artifacts (except `/target`)
❌ Personal or test files
❌ Empty directory structures

## Compliance Check

Run the compliance tool to verify structure:
```bash
cargo run -p sq-compliance
```

This ensures:
- No unwrap() calls
- Proper error handling
- Code quality standards
- Architectural alignment