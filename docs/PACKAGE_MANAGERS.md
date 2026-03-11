# Package Managers and Distribution Surfaces

CURD now has a release-generation path for multiple package ecosystems.

Generated assets are emitted under:

- `dist/package-managers/`

The generator is:

- [generate_package_manager_assets.py](../scripts/generate_package_manager_assets.py)

## What gets generated

### Native OS package managers

- Homebrew formula
- Winget manifests
- Scoop manifest
- Chocolatey package skeleton

These use the release binary artifacts and their checksums when available.

### Wrapper and launcher ecosystems

Generated snippets are also included for:

- `uvx`
- `pipx`
- `bunx`
- `npx`
- `mise`
- `pixi`

Those are intended to launch CURD through the published Python or Node wrappers where that is the better fit.

## Human-facing download links

For docs published to `curd.aerospeedsta.dev`, use GitHub's latest-release download URLs instead of version-pinned ones.

Examples:

```text
https://github.com/bharath/CURD/releases/latest/download/curd-linux-x86_64.tar.gz
https://github.com/bharath/CURD/releases/latest/download/curd-linux-aarch64.tar.gz
https://github.com/bharath/CURD/releases/latest/download/curd-linux-x86_64-static.tar.gz
https://github.com/bharath/CURD/releases/latest/download/curd-linux-aarch64-static.tar.gz
https://github.com/bharath/CURD/releases/latest/download/curd-win-x64.zip
https://github.com/bharath/CURD/releases/latest/download/curd-win-arm64.zip
```

Use version-pinned links only in generated manifests and package-manager submission assets.

## How to generate

```bash
make dist-package-managers
```

or directly:

```bash
python3 scripts/generate_package_manager_assets.py --output dist/package-managers --dist-dir dist
```

## Release URL base

By default the generator assumes GitHub release URLs of the form:

```text
https://github.com/bharath/CURD/releases/download/v<version>/
```

Override that during release generation with:

```bash
CURD_RELEASE_BASE_URL="https://your-host/releases/v0.7.1-beta" make dist-package-managers
```

For public docs, prefer the latest-release URL style instead:

```text
https://github.com/bharath/CURD/releases/latest/download/
```

## Why this split exists

Some ecosystems want a real native package:

- Homebrew
- Winget
- Scoop
- Chocolatey

Others are better served through an existing wrapper package:

- Python launchers through `curd-python`
- Node launchers through `curd-node`

That keeps the install story broad without pretending every ecosystem should consume CURD the same way.
