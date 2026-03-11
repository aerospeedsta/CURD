#!/usr/bin/env python3
from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import textwrap


ROOT = Path(__file__).resolve().parent.parent
DEFAULT_OUTPUT = ROOT / "dist" / "package-managers"
CURD_CARGO = ROOT / "curd" / "Cargo.toml"
PYPROJECT = ROOT / "curd-python" / "pyproject.toml"
NODE_PACKAGE = ROOT / "curd-node" / "package.json"


def read_version() -> str:
    for line in CURD_CARGO.read_text().splitlines():
        line = line.strip()
        if line.startswith("version ="):
            return line.split('"')[1]
    raise RuntimeError("could not determine CURD version from curd/Cargo.toml")


def file_sha256(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def maybe_sha256(path: Path) -> str:
    return file_sha256(path) if path.exists() else "REPLACE_WITH_SHA256"


def write(path: Path, content: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(content)


def release_base_url(version: str) -> str:
    default = f"https://github.com/bharath/CURD/releases/download/v{version}"
    return os.environ.get("CURD_RELEASE_BASE_URL", default).rstrip("/")


def render_homebrew(version: str, base_url: str, dist_dir: Path) -> str:
    archive = dist_dir / "cli" / "macos" / "curd-macos-universal.tar.gz"
    sha = maybe_sha256(archive)
    return textwrap.dedent(
        f"""\
        class Curd < Formula
          desc "Semantic code intelligence control plane for humans and agents"
          homepage "https://github.com/bharath/CURD"
          url "{base_url}/curd-macos-universal.tar.gz"
          sha256 "{sha}"
          license "GPL-3.0-only"
          version "{version}"

          def install
            bin.install "curd"
          end

          test do
            system "#{{bin}}/curd", "--version"
          end
        end
        """
    )


def render_scoop(version: str, base_url: str, dist_dir: Path) -> str:
    archive = dist_dir / "cli" / "windows" / "curd-win-x64.zip"
    sha = maybe_sha256(archive)
    manifest = {
        "version": version,
        "description": "Semantic code intelligence control plane for humans and agents",
        "homepage": "https://github.com/bharath/CURD",
        "license": "GPL-3.0-only",
        "url": f"{base_url}/curd-win-x64.zip",
        "hash": sha,
        "bin": "curd-x64.exe",
        "checkver": {"github": "https://github.com/bharath/CURD"},
        "autoupdate": {
            "url": "https://github.com/bharath/CURD/releases/download/v$version/curd-win-x64.zip"
        },
    }
    return json.dumps(manifest, indent=2) + "\n"


def render_choco_nuspec(version: str) -> str:
    return textwrap.dedent(
        f"""\
        <?xml version="1.0"?>
        <package >
          <metadata>
            <id>curd</id>
            <version>{version}</version>
            <title>CURD</title>
            <authors>Aerospeedsta</authors>
            <projectUrl>https://github.com/bharath/CURD</projectUrl>
            <licenseUrl>https://www.gnu.org/licenses/gpl-3.0.html</licenseUrl>
            <requireLicenseAcceptance>false</requireLicenseAcceptance>
            <description>Semantic code intelligence control plane for humans and agents.</description>
            <tags>curd code-intelligence mcp agentic</tags>
          </metadata>
        </package>
        """
    )


def render_choco_install(version: str, base_url: str, dist_dir: Path) -> str:
    archive = dist_dir / "cli" / "windows" / "curd-win-x64.zip"
    sha = maybe_sha256(archive)
    return textwrap.dedent(
        f"""\
        $ErrorActionPreference = 'Stop'

        $packageName = 'curd'
        $url64 = '{base_url}/curd-win-x64.zip'
        $checksum64 = '{sha}'
        $checksumType64 = 'sha256'

        Install-ChocolateyZipPackage `
          -PackageName $packageName `
          -Url64bit $url64 `
          -UnzipLocation "$(Split-Path -Parent $MyInvocation.MyCommand.Definition)" `
          -Checksum64 $checksum64 `
          -ChecksumType64 $checksumType64
        """
    )


def render_choco_uninstall() -> str:
    return textwrap.dedent(
        """\
        $toolsDir = Split-Path -Parent $MyInvocation.MyCommand.Definition
        Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $toolsDir 'curd-x64.exe')
        Remove-Item -Force -ErrorAction SilentlyContinue (Join-Path $toolsDir 'install.ps1')
        """
    )


def render_winget_version(version: str) -> str:
    parts = []
    for chunk in version.replace("-beta", ".0").replace("-", ".").split("."):
        if chunk.isdigit():
            parts.append(chunk)
        else:
            parts.append("0")
    while len(parts) < 3:
        parts.append("0")
    return ".".join(parts[:4])


def render_winget_manifests(version: str, base_url: str, dist_dir: Path) -> dict[str, str]:
    winget_version = render_winget_version(version)
    zip_path = dist_dir / "cli" / "windows" / "curd-win-x64.zip"
    sha = maybe_sha256(zip_path).upper()
    identifier = "Aerospeedsta.CURD"
    manifest_version = "1.6.0"
    base = f"{identifier}/{winget_version}"
    return {
        f"{base}/{identifier}.yaml": textwrap.dedent(
            f"""\
            PackageIdentifier: {identifier}
            PackageVersion: {winget_version}
            DefaultLocale: en-US
            ManifestType: version
            ManifestVersion: {manifest_version}
            """
        ),
        f"{base}/{identifier}.installer.yaml": textwrap.dedent(
            f"""\
            PackageIdentifier: {identifier}
            PackageVersion: {winget_version}
            InstallerType: zip
            NestedInstallerType: portable
            NestedInstallerFiles:
              - RelativeFilePath: curd-x64.exe
                PortableCommandAlias: curd
            Installers:
              - Architecture: x64
                InstallerUrl: {base_url}/curd-win-x64.zip
                InstallerSha256: {sha}
            ManifestType: installer
            ManifestVersion: {manifest_version}
            """
        ),
        f"{base}/{identifier}.locale.en-US.yaml": textwrap.dedent(
            f"""\
            PackageIdentifier: {identifier}
            PackageVersion: {winget_version}
            PackageLocale: en-US
            Publisher: Aerospeedsta
            PackageName: CURD
            ShortDescription: Semantic code intelligence control plane for humans and agents
            License: GPL-3.0-only
            Homepage: https://github.com/bharath/CURD
            ManifestType: defaultLocale
            ManifestVersion: {manifest_version}
            """
        ),
    }


def render_pixi_snippet(py_version: str) -> str:
    return textwrap.dedent(
        f"""\
        [project]
        name = "curd-shell"
        channels = ["conda-forge"]
        platforms = ["linux-64", "osx-arm64", "osx-64", "win-64"]

        [dependencies]
        python = "{py_version}"
        pip = "*"

        [tasks]
        curd = "python -m curd_python.cli"

        [pypi-dependencies]
        curd-python = "=={read_python_version()}"
        """
    )


def read_python_version() -> str:
    for line in PYPROJECT.read_text().splitlines():
        line = line.strip()
        if line.startswith("version ="):
            return line.split('"')[1]
    raise RuntimeError("could not determine curd-python version")


def read_node_version() -> str:
    data = json.loads(NODE_PACKAGE.read_text())
    return data["version"]


def render_mise_snippet() -> str:
    py_version = read_python_version()
    node_version = read_node_version()
    return textwrap.dedent(
        f"""\
        [env]
        _.python.venv = ".venv"

        [tools]
        python = "3.13"
        node = "22"
        uv = "latest"
        bun = "latest"

        [tasks.curd-uvx]
        run = "uvx --from curd-python=={py_version} curd --version"

        [tasks.curd-bunx]
        run = "bunx --bun curd-node@{node_version} --version"
        """
    )


def render_install_commands(version: str) -> str:
    py_version = read_python_version()
    node_version = read_node_version()
    return textwrap.dedent(
        f"""\
        # CURD Package Manager Commands

        This file collects practical install commands for ecosystems that should use
        the published Python or Node wrappers instead of a native OS package.

        ## Python-first launchers

        ```bash
        uvx --from curd-python=={py_version} curd --version
        pipx install curd-python=={py_version}
        ```

        ## Node-first launchers

        ```bash
        bunx --bun curd-node@{node_version} --version
        npx curd-node@{node_version} --version
        ```

        ## Mise examples

        See `mise/mise.toml` in this directory.

        ## Pixi example

        See `pixi/pixi.toml` in this directory.

        ## Native OS package managers

        Generated manifests are included for:

        - Homebrew
        - Winget
        - Scoop
        - Chocolatey

        Version: `{version}`
        """
    )


def main() -> None:
    parser = argparse.ArgumentParser(description="Generate package manager release assets for CURD.")
    parser.add_argument("--output", type=Path, default=DEFAULT_OUTPUT)
    parser.add_argument("--dist-dir", type=Path, default=ROOT / "dist")
    args = parser.parse_args()

    version = read_version()
    base_url = release_base_url(version)
    output = args.output
    dist_dir = args.dist_dir

    write(output / "homebrew" / "curd.rb", render_homebrew(version, base_url, dist_dir))
    write(output / "scoop" / "curd.json", render_scoop(version, base_url, dist_dir))
    write(output / "chocolatey" / "curd.nuspec", render_choco_nuspec(version))
    write(
        output / "chocolatey" / "tools" / "chocolateyinstall.ps1",
        render_choco_install(version, base_url, dist_dir),
    )
    write(
        output / "chocolatey" / "tools" / "chocolateyuninstall.ps1",
        render_choco_uninstall(),
    )
    for rel_path, content in render_winget_manifests(version, base_url, dist_dir).items():
        write(output / "winget" / rel_path, content)
    write(output / "pixi" / "pixi.toml", render_pixi_snippet(">=3.13,<3.14"))
    write(output / "mise" / "mise.toml", render_mise_snippet())
    write(output / "INSTALL_COMMANDS.md", render_install_commands(version))


if __name__ == "__main__":
    main()
