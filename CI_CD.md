# CI/CD Documentation

This document explains the continuous integration and deployment setup for the nodes-manager project.

## Overview

The project uses GitHub Actions for automated testing, building, and releasing. All workflows are located in `.github/workflows/`.

## Workflows

### 1. Test Workflow (`test.yml`)

**Triggers:**
- Push to `main` or `develop` branches
- Pull requests to `main` or `develop` branches

**What it does:**
- ✅ Runs on Ubuntu and macOS
- ✅ Checks code formatting (`cargo fmt`)
- ✅ Runs Clippy lints (`cargo clippy`)
- ✅ Builds the project
- ✅ Runs all 95 tests
- ✅ Generates code coverage report
- ✅ Runs security audit

**Jobs:**
1. **test**: Runs tests on multiple platforms
2. **coverage**: Generates code coverage with tarpaulin
3. **security-audit**: Checks for known vulnerabilities

### 2. Pull Request Workflow (`pr.yml`)

**Triggers:**
- Pull request opened, synchronized, or reopened

**What it does:**
- ✅ Quick formatting and lint checks
- ✅ Runs tests on Ubuntu and macOS
- ✅ Builds debug and release binaries
- ✅ Verifies binary sizes
- ✅ Provides test summary

**Jobs:**
1. **check**: Fast formatting and clippy checks
2. **test**: Full test suite on multiple OS
3. **build**: Build verification
4. **test-summary**: Aggregates results

### 3. Release Workflow (`release.yml`)

**Triggers:**
- Git tags matching `v*.*.*` (e.g., `v1.0.0`)
- Manual workflow dispatch

**What it does:**
- ✅ Creates GitHub release
- ✅ Builds binaries for multiple platforms
- ✅ Generates checksums for verification
- ✅ Builds and pushes Docker images (optional)

**Platforms supported:**
- Linux x86_64 (`manager-linux-amd64`, `agent-linux-amd64`)
- macOS x86_64 (`manager-macos-amd64`, `agent-macos-amd64`)
- macOS ARM64/Apple Silicon (`manager-macos-arm64`, `agent-macos-arm64`)

**Artifacts:**
- Compressed binaries (`.tar.gz`)
- SHA256 checksums (`.tar.gz.sha256`)
- Docker images (if configured)

### 4. Badge Update Workflow (`badges.yml`)

**Triggers:**
- Push to `main` branch
- Weekly schedule (Sunday at midnight)

**What it does:**
- Updates test count badges
- Keeps README badges current

## Creating a Release

### Step 1: Prepare the Release

1. Update version numbers if needed
2. Update CHANGELOG.md with release notes
3. Ensure all tests pass locally:
   ```bash
   cargo test
   ```

### Step 2: Create and Push a Tag

```bash
# Create a new tag
git tag -a v1.0.0 -m "Release version 1.0.0"

# Push the tag to GitHub
git push origin v1.0.0
```

### Step 3: Monitor the Release

1. Go to **Actions** tab on GitHub
2. Watch the "Release" workflow
3. Once complete, check the **Releases** page

### Step 4: Download and Verify Binaries

```bash
# Download binary
wget https://github.com/YOUR_USERNAME/nodes-manager/releases/download/v1.0.0/manager-linux-amd64.tar.gz

# Download checksum
wget https://github.com/YOUR_USERNAME/nodes-manager/releases/download/v1.0.0/manager-linux-amd64.tar.gz.sha256

# Verify checksum
sha256sum -c manager-linux-amd64.tar.gz.sha256

# Extract binary
tar xzf manager-linux-amd64.tar.gz

# Make executable and move to PATH
chmod +x manager
sudo mv manager /usr/local/bin/
```

## Docker Images

### Building Docker Images Locally

```bash
# Build manager image
docker build -f Dockerfile.manager -t nodes-manager:latest .

# Build agent image
docker build -f Dockerfile.agent -t nodes-agent:latest .
```

### Running with Docker

```bash
# Run manager
docker run -d \
  -p 8080:8080 \
  -v $(pwd)/config:/app/config \
  -v $(pwd)/data:/app/data \
  --name nodes-manager \
  nodes-manager:latest

# Run agent
docker run -d \
  -p 8745:8745 \
  -e AGENT_API_KEY="your-secure-key" \
  --name nodes-agent \
  nodes-agent:latest
```

### Automated Docker Builds

If you want to automatically push Docker images to Docker Hub:

1. Add secrets to GitHub repository:
   - `DOCKER_USERNAME`: Your Docker Hub username
   - `DOCKER_PASSWORD`: Your Docker Hub password or access token

2. The release workflow will automatically build and push images

## Code Coverage

### Setting Up Codecov (Optional)

1. Sign up at [codecov.io](https://codecov.io)
2. Add your repository
3. Add `CODECOV_TOKEN` to GitHub repository secrets
4. Coverage reports will be uploaded automatically

### Viewing Coverage Locally

```bash
# Install tarpaulin
cargo install cargo-tarpaulin

# Generate coverage report
cargo tarpaulin --verbose --all-features --workspace --timeout 120 --out Html

# Open coverage report
open tarpaulin-report.html
```

## Security Audits

The security audit workflow uses [cargo-audit](https://github.com/RustSec/rustsec) to check for known vulnerabilities.

### Running Locally

```bash
# Install cargo-audit
cargo install cargo-audit

# Run audit
cargo audit
```

## Caching Strategy

All workflows use GitHub Actions caching to speed up builds:

- **Cargo registry**: `~/.cargo/registry`
- **Cargo index**: `~/.cargo/git`
- **Build artifacts**: `target/`

This significantly reduces build times after the first run.

## Troubleshooting

### Tests Fail in CI but Pass Locally

1. Check if tests are platform-specific
2. Verify all dependencies are available in CI environment
3. Check for race conditions in concurrent tests
4. Use `serial_test` crate for tests that need sequential execution

### Release Workflow Fails

1. Ensure tag follows `v*.*.*` format
2. Check that all tests pass before creating tag
3. Verify GitHub token has necessary permissions
4. Check build logs for specific errors

### Docker Build Fails

1. Ensure Dockerfiles are in repository root
2. Check that all dependencies are available in Debian image
3. Verify Rust version compatibility
4. Test Docker build locally first

### Coverage Report Not Uploading

1. Verify `CODECOV_TOKEN` is set in repository secrets
2. Check that tarpaulin installation succeeded
3. Ensure coverage report file exists (`cobertura.xml`)
4. Check Codecov service status

## Best Practices

### Before Merging PRs

1. Ensure all PR checks pass
2. Review test coverage changes
3. Check for new security vulnerabilities
4. Verify code formatting and lints

### Before Creating Releases

1. Run full test suite locally
2. Test binaries on target platforms
3. Update documentation
4. Write clear release notes

### Security

1. Never commit secrets to repository
2. Use GitHub secrets for sensitive data
3. Regularly update dependencies
4. Monitor security audit results

## Workflow Files

```
.github/workflows/
├── test.yml       # Main test workflow (runs on push/PR)
├── pr.yml         # Pull request specific checks
├── release.yml    # Release and binary building
└── badges.yml     # Badge updates
```

## Environment Variables

### Test Workflow
- `CARGO_TERM_COLOR=always`: Colorized output
- `RUST_BACKTRACE=1`: Full backtraces on panic

### Release Workflow
- `GITHUB_TOKEN`: Automatically provided by GitHub
- `DOCKER_USERNAME`: (Optional) Docker Hub username
- `DOCKER_PASSWORD`: (Optional) Docker Hub password/token
- `CODECOV_TOKEN`: (Optional) Codecov upload token

## Performance Metrics

- **Test execution**: ~10-30 seconds (with cache)
- **Full build**: ~2-5 minutes (with cache)
- **Release build**: ~10-15 minutes (6 binaries)
- **Docker build**: ~5-10 minutes per image

## Monitoring

### GitHub Actions Dashboard

Monitor workflow runs at:
```
https://github.com/YOUR_USERNAME/nodes-manager/actions
```

### Key Metrics

- ✅ Test pass rate (should be 100%)
- ✅ Build success rate
- ✅ Average build time
- ✅ Cache hit rate
- ✅ Security audit status

## Future Enhancements

Potential improvements to consider:

1. **Automated dependency updates** with Dependabot
2. **Benchmark tracking** over time
3. **Integration tests** with real agent/manager
4. **Performance regression detection**
5. **Automated changelog generation**
6. **Multi-architecture Docker images**
7. **Release notes automation**

## Support

For issues with CI/CD:
1. Check workflow logs in Actions tab
2. Review this documentation
3. Open an issue on GitHub
4. Check GitHub Actions status page

---

**Last Updated**: 2025-01-09
**Test Count**: 95 tests
**Supported Platforms**: Linux (x86_64), macOS (x86_64, ARM64)
