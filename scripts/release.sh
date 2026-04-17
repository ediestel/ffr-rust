#!/bin/bash
# Tag + push a release. Downstream CI (.github/workflows/release.yml) builds
# and publishes artifacts for the pushed tag.
set -euo pipefail

VERSION="${1:?Usage: ./scripts/release.sh <version> (e.g. 0.2.0)}"
VERSION="${VERSION#v}"
TAG="v${VERSION}"

git pull

if ! git diff --quiet || ! git diff --cached --quiet; then
  echo "Error: Working tree is not clean. Commit or stash changes first."
  exit 1
fi

if git rev-parse "$TAG" >/dev/null 2>&1; then
  echo "Error: Tag $TAG already exists"
  exit 1
fi

echo "Releasing $VERSION..."

echo "→ Updating Cargo.toml versions to $VERSION"
if ! cargo set-version "$VERSION" 2>/dev/null; then
  echo "cargo-edit not found, installing..."
  cargo install cargo-edit
  cargo set-version "$VERSION"
fi

git add -A
git commit -m "chore: release $VERSION"

echo "→ Creating tag $TAG"
git tag -a "$TAG" -m "Release $VERSION"

echo "→ Pushing to origin"
git push origin
git push origin "$TAG"

echo
echo "Release $VERSION created and pushed."
echo "CI will build and publish the binaries."
