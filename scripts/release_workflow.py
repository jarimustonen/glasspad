#!/usr/bin/env python3
"""Generate/check the approved Glasspad override of cargo-dist's release workflow.

cargo-dist does not support a pre-install hook for build-local-artifacts. Its
allow-dirty = ["ci"] exempts the workflow from its native generation check, so
this command generates a pristine CI file in a disposable workspace and applies
only our pinned macOS override before comparison. Never generate in the live
workspace: that would overwrite the override before it was checked.

Usage: python3 scripts/release_workflow.py --check [--dist /path/to/dist]
       python3 scripts/release_workflow.py --write [--dist /path/to/dist]
"""
import argparse
from pathlib import Path
import shutil
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
WORKFLOW = Path('.github/workflows/release.yml')
ORIGINAL = """      - name: Install dist
        run: ${{ matrix.install_dist.run }}
      # Get the dist-manifest"""
REPLACEMENT = """      # This is the Glasspad-approved cargo-dist CI override: upstream generates
      # an unscoped installer here. Only the self-hosted macOS job needs isolation.
      # Keep Linux on the generated matrix installer and the plan cache untouched.
      - name: Install dist (self-hosted macOS)
        if: ${{ runner.os == 'macOS' }}
        shell: bash
        run: |
          set -euo pipefail
          : "${RUNNER_TEMP:?RUNNER_TEMP must be set for isolated dist install}"
          # mktemp is atomic even if two release jobs share a runner/temp directory.
          export CARGO_DIST_INSTALL_DIR="$(mktemp -d "$RUNNER_TEMP/glasspad-dist.XXXXXXXX")"
          export CARGO_DIST_NO_MODIFY_PATH=1
          ${{ matrix.install_dist.run }}
          # Verify the installer did not fall back to ~/.cargo/bin (or an older dist).
          export PATH="$CARGO_DIST_INSTALL_DIR/bin:$PATH"
          test "$(command -v dist)" = "$CARGO_DIST_INSTALL_DIR/bin/dist"
          echo "$CARGO_DIST_INSTALL_DIR/bin" >> "$GITHUB_PATH"
      - name: Install dist (hosted Linux)
        if: ${{ runner.os != 'macOS' }}
        run: ${{ matrix.install_dist.run }}
      # Get the dist-manifest"""


def generate(dist: str) -> str:
    # A copy outside the parent Cargo workspace is required: dist resolves the
    # workspace root via Cargo metadata, even when run in a nested directory.
    with tempfile.TemporaryDirectory(prefix='glasspad-dist-generate-') as tmp:
        with subprocess.Popen(['git', 'archive', 'HEAD'], cwd=ROOT, stdout=subprocess.PIPE) as git:
            subprocess.run(['tar', '-xf', '-', '-C', tmp], stdin=git.stdout, check=True)
            git.stdout.close()
            if git.wait() != 0:
                raise RuntimeError('git archive failed')
        for file in ('Cargo.toml', 'Cargo.lock'):
            shutil.copy2(ROOT / file, Path(tmp) / file)
        config = (ROOT / 'dist-workspace.toml').read_text()
        marker = 'allow-dirty = ["ci"]\n'
        if config.count(marker) != 1:
            raise RuntimeError('expected exactly one approved cargo-dist allow-dirty override')
        (Path(tmp) / 'dist-workspace.toml').write_text(config.replace(marker, ''))
        subprocess.run([dist, 'generate', '--mode', 'ci'], cwd=tmp, check=True)
        generated = (Path(tmp) / WORKFLOW).read_text()
        if generated.count(ORIGINAL) != 1:
            raise RuntimeError('cargo-dist local installer template changed; review override')
        return generated.replace(ORIGINAL, REPLACEMENT)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    operation = parser.add_mutually_exclusive_group(required=True)
    operation.add_argument('--check', action='store_true')
    operation.add_argument('--write', action='store_true')
    parser.add_argument('--dist', default='dist', help='path to pinned cargo-dist 0.33.0')
    args = parser.parse_args()
    dist = shutil.which(args.dist)
    if not dist:
        parser.error(f'dist binary not found: {args.dist}')
    expected = generate(str(Path(dist).resolve()))
    path = ROOT / WORKFLOW
    if args.write:
        path.write_text(expected)
        print(f'generated {WORKFLOW}')
    elif path.read_text() != expected:
        raise SystemExit(f'{WORKFLOW} differs from cargo-dist generation + macOS override; run --write')
    else:
        print(f'{WORKFLOW}: generation + macOS override match')


if __name__ == '__main__':
    main()
