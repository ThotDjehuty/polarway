# ReadTheDocs Setup Guide
## Polarway Documentation Hosting

This guide explains how to activate and configure ReadTheDocs hosting for Polarway documentation.

## Prerequisites

✅ Configuration files already in repository:
- `.readthedocs.yml` - Build configuration
- `docs/requirements.txt` - Python dependencies
- `docs/source/conf.py` - Sphinx configuration
- Complete documentation in `docs/source/`

## Activation Steps

### 1. Import Project to ReadTheDocs

1. Visit https://readthedocs.org/dashboard/import/
2. Click **"Import a Project"**
3. Select **"Import Manually"**
4. Fill in project details:
   - **Name**: `polarway`
   - **Repository URL**: `https://github.com/ThotDjehuty/polarway`
   - **Repository type**: Git
   - **Default branch**: `main`
   - **Default version**: `latest`
   - **Programming Language**: Python

### 2. Configure Advanced Settings

1. Go to **Admin → Advanced Settings**
2. Enable:
   - ✅ **Build pull requests for this project**
   - ✅ **Install your project inside a virtualenv**
   - ✅ **Enable PDF build**
   - ✅ **Enable EPUB build**
3. **Python interpreter**: `CPython 3.11`
4. **Documentation type**: `Sphinx Html`

### 3. Build Configuration

Our `.readthedocs.yml` automatically configures:

```yaml
- Ubuntu 22.04 build environment
- Python 3.11 interpreter
- Rust toolchain installation (for cargo doc)
- Sphinx with RTD theme
- PDF and EPUB output formats
- Automatic dependency installation from docs/requirements.txt
```

### 4. Trigger First Build

1. Go to **Builds** tab
2. Click **"Build Version: latest"**
3. Monitor build logs for errors
4. Expected build time: ~5-8 minutes (includes Rust compilation)

### 5. Verify Build Success

Once build completes, documentation will be available at:

```
https://polarway.readthedocs.io/en/latest/
```

**Key pages to verify:**
- Home: `https://polarway.readthedocs.io/en/latest/`
- Storage Guide: `https://polarway.readthedocs.io/en/latest/storage.html`
- Architecture: `https://polarway.readthedocs.io/en/latest/architecture.html`
- API Reference: `https://polarway.readthedocs.io/en/latest/api.html`

### 6. Configure Webhooks (Automatic)

ReadTheDocs automatically sets up GitHub webhooks for:
- ✅ Push to `main` → Rebuild documentation
- ✅ New tag created → Build versioned docs
- ✅ Pull request opened → Build PR preview

## Versioning Strategy

### Tag-based Versions

Create documentation for specific releases:

```bash
git tag -a v0.53.0 -m "Release v0.53.0: Hybrid storage layer"
git push origin v0.53.0
```

ReadTheDocs will automatically:
1. Detect new tag
2. Build documentation for that version
3. Make it available at `https://polarway.readthedocs.io/en/v0.53.0/`

### Branch-based Versions

Additional branches to document:
- `main` → https://polarway.readthedocs.io/en/latest/
- `develop` → https://polarway.readthedocs.io/en/develop/
- `stable` → https://polarway.readthedocs.io/en/stable/

## Troubleshooting

### Build Fails: Rust Not Found

**Error**: `cargo: command not found`

**Solution**: Already handled in `.readthedocs.yml` via `post_create_environment` job

### Build Fails: Missing Dependencies

**Error**: `ModuleNotFoundError: No module named 'polars'`

**Solution**: Add missing package to `docs/requirements.txt`

### Build Timeout (>15 minutes)

**Cause**: Rust compilation too slow

**Solution**: 
1. Reduce cargo doc scope: `cargo doc --no-deps --lib`
2. Or disable Rust docs temporarily: Comment out `post_install` in `.readthedocs.yml`

### PDF Build Fails: Non-ASCII Characters

**Error**: `LaTeX Error: Unicode character`

**Solution**: Already configured in `.readthedocs.yml` with fail_on_warning: false

## Custom Domain (Optional)

To serve docs from custom domain (e.g., `docs.polarway.io`):

1. Go to **Admin → Domains**
2. Add custom domain: `docs.polarway.io`
3. Configure DNS CNAME:
   ```
   docs.polarway.io  CNAME  polarway.readthedocs.io
   ```
4. Wait for DNS propagation (~24 hours)

## Badge for README

Add ReadTheDocs badge to README.md:

```markdown
[![Documentation Status](https://readthedocs.org/projects/polarway/badge/?version=latest)](https://polarway.readthedocs.io/en/latest/?badge=latest)
```

## Support

- ReadTheDocs Docs: https://docs.readthedocs.io/
- Build Logs: https://readthedocs.org/projects/polarway/builds/
- Community: https://readthedocs.org/support/

---

**Status**: Configuration complete ✅  
**Next**: Activate project on ReadTheDocs dashboard  
**ETA**: 10 minutes to full deployment
