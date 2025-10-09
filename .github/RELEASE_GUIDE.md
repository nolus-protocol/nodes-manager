# Release Guide

Quick guide for creating releases with automated binary builds.

## Prerequisites

- Push access to the repository
- GitHub repository set up with workflows
- Local repository with all changes committed

## Quick Release Steps

### 1. Verify Everything Works

```bash
# Run all tests
cargo test

# Build release binaries locally
cargo build --release

# Check binary sizes
ls -lh target/release/manager target/release/agent
```

### 2. Update Version (if needed)

Update version in `Cargo.toml` files:

```toml
# manager/Cargo.toml
[package]
name = "manager"
version = "1.0.0"  # <-- Update this

# agent/Cargo.toml
[package]
name = "agent"
version = "1.0.0"  # <-- Update this
```

### 3. Commit Changes

```bash
git add .
git commit -m "chore: bump version to 1.0.0"
git push origin main
```

### 4. Create and Push Tag

```bash
# Create annotated tag
git tag -a v1.0.0 -m "Release v1.0.0

- Add feature X
- Fix bug Y
- Improve performance Z"

# Push tag to GitHub
git push origin v1.0.0
```

### 5. Monitor Release Build

1. Go to: `https://github.com/YOUR_USERNAME/nodes-manager/actions`
2. Click on the "Release" workflow run
3. Watch the build progress (takes ~10-15 minutes)

### 6. Verify Release

1. Go to: `https://github.com/YOUR_USERNAME/nodes-manager/releases`
2. Check that the release was created with all binaries:
   - `manager-linux-amd64.tar.gz`
   - `manager-macos-amd64.tar.gz`
   - `manager-macos-arm64.tar.gz`
   - `agent-linux-amd64.tar.gz`
   - `agent-macos-amd64.tar.gz`
   - `agent-macos-arm64.tar.gz`
3. Verify checksums are present

### 7. Test Downloaded Binary

```bash
# Download
wget https://github.com/YOUR_USERNAME/nodes-manager/releases/download/v1.0.0/manager-linux-amd64.tar.gz

# Verify checksum
wget https://github.com/YOUR_USERNAME/nodes-manager/releases/download/v1.0.0/manager-linux-amd64.tar.gz.sha256
sha256sum -c manager-linux-amd64.tar.gz.sha256

# Extract and test
tar xzf manager-linux-amd64.tar.gz
./manager --version  # or whatever verification you need
```

## Release Checklist

- [ ] All tests passing locally (`cargo test`)
- [ ] Version updated in Cargo.toml files (if applicable)
- [ ] Changes committed and pushed to main
- [ ] Tag created with proper format (`v*.*.*`)
- [ ] Tag pushed to GitHub
- [ ] Release workflow completed successfully
- [ ] All binaries present in release
- [ ] Checksums verified
- [ ] Release notes updated (optional but recommended)

## Troubleshooting

### Tag already exists

```bash
# Delete local tag
git tag -d v1.0.0

# Delete remote tag
git push origin :refs/tags/v1.0.0

# Create new tag
git tag -a v1.0.0 -m "Release v1.0.0"
git push origin v1.0.0
```

### Build failed

1. Check the Actions tab for error logs
2. Common issues:
   - Tests failing on specific platform
   - Dependency issues
   - Clippy warnings treated as errors
3. Fix the issue, delete the tag, and retry

### Missing binaries

1. Check if all build jobs completed
2. Verify GitHub token permissions
3. Check upload step logs in workflow

## Advanced: Docker Images (Optional)

### Enable Docker Builds

1. Create Docker Hub account
2. Add secrets to GitHub repository:
   ```
   Settings → Secrets → Actions → New repository secret
   
   Name: DOCKER_USERNAME
   Value: your-dockerhub-username
   
   Name: DOCKER_PASSWORD
   Value: your-dockerhub-token
   ```

3. Next release will automatically push Docker images

### Manual Docker Build

```bash
# Build images
docker build -f Dockerfile.manager -t your-username/nodes-manager:v1.0.0 .
docker build -f Dockerfile.agent -t your-username/nodes-agent:v1.0.0 .

# Push images
docker push your-username/nodes-manager:v1.0.0
docker push your-username/nodes-agent:v1.0.0
```

## Version Numbering

Follow [Semantic Versioning](https://semver.org/):

- **MAJOR** (`1.0.0` → `2.0.0`): Breaking changes
- **MINOR** (`1.0.0` → `1.1.0`): New features, backwards compatible
- **PATCH** (`1.0.0` → `1.0.1`): Bug fixes, backwards compatible

## Pre-releases

For testing releases:

```bash
# Create pre-release tag
git tag -a v1.0.0-rc.1 -m "Release candidate 1"
git push origin v1.0.0-rc.1
```

Note: Pre-release tags will still trigger the release workflow. You can manually mark them as pre-release in GitHub.

## Automated Release Notes

The release workflow includes basic release notes. For better notes:

1. Manually edit the release on GitHub
2. Add changelog entries
3. Highlight breaking changes
4. Include upgrade instructions

## Rolling Back a Release

If you need to remove a release:

1. Go to Releases page
2. Click on the release
3. Click "Delete" button
4. Delete the tag:
   ```bash
   git push origin :refs/tags/v1.0.0
   git tag -d v1.0.0
   ```

## Quick Commands Reference

```bash
# Create release
git tag -a v1.0.0 -m "Release v1.0.0" && git push origin v1.0.0

# Delete release tag
git tag -d v1.0.0 && git push origin :refs/tags/v1.0.0

# List all tags
git tag -l

# Show tag details
git show v1.0.0

# Download release binary
wget https://github.com/USER/REPO/releases/download/v1.0.0/BINARY.tar.gz

# Verify checksum
sha256sum -c BINARY.tar.gz.sha256
```

---

**Need Help?**
- Check [CI_CD.md](../CI_CD.md) for detailed documentation
- Review workflow files in `.github/workflows/`
- Open an issue if you encounter problems
