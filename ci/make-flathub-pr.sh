#!/bin/bash
set -xe
TAG_NAME=$(ci/tag-name.sh)

python3 -m pip install toml aiohttp
curl -L 'https://github.com/flatpak/flatpak-builder-tools/raw/master/cargo/flatpak-cargo-generator.py' > /tmp/flatpak-cargo-generator.py
python3 /tmp/flatpak-cargo-generator.py Cargo.lock -o flathub/generated-sources.json

URL="https://github.com/shelldone/shelldone/releases/download/${TAG_NAME}/shelldone-${TAG_NAME}-src.tar.gz"

# We require that something has obtained the source archive already and left it
# in the current dir. This is handled by actions/download-artifact in CI
SHA256=$(sha256sum shelldone*-src.tar.gz | cut -d' ' -f1)

sed -e "s,@URL@,$URL,g" -e "s/@SHA256@/$SHA256/g" < assets/flatpak/net.shelldone.terminal.template.json > flathub/net.shelldone.terminal.json

RELEASE_DATE=$(git -c "core.abbrev=8" show -s "--format=%cd" "--date=format:%Y-%m-%d")
sed -e "s,@TAG_NAME@,$TAG_NAME,g" -e "s/@DATE@/$RELEASE_DATE/g" < assets/flatpak/net.shelldone.terminal.appdata.template.xml > flathub/net.shelldone.terminal.appdata.xml

cd flathub
git config user.email team@shelldone.dev
git config user.name 'Shelldone Labs'
git checkout -b "$TAG_NAME" origin/master
git add --all
git diff --cached
git commit -m "New version: $TAG_NAME"
git push --set-upstream origin "$TAG_NAME" --quiet
