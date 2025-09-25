#!/bin/bash

SERVE=no
if [ "$1" == "serve" ] ; then
  SERVE=yes
fi

for util in gelatyx ; do
  if ! hash $util 2>/dev/null ; then
    cargo install $util --locked
  fi
done

tracked_markdown=$(mktemp)
trap "rm ${tracked_markdown}" "EXIT"
find docs -type f | egrep '\.(markdown|md)$' > $tracked_markdown

gelatyx --language lua --file-list $tracked_markdown --language-config ci/stylua.toml
gelatyx --language lua --file-list $tracked_markdown --language-config ci/stylua.toml --check || exit 1

set -ex

# Use the GH CLI to make an authenticated request if available,
# otherwise just do an ad-hoc curl.
# However, if we are called from within a GH actions workflow (BUILD_REASON
# is set), only use `gh` if GH_TOKEN is also set, otherwise it will refuse
# to run.
function ghapi() {
  if hash gh 2>/dev/null && test \( -n "$BUILD_REASON" -a -n "$GH_TOKEN" \) -o -z "$BUILD_REASON"; then
    gh api $1
  else
    curl https://api.github.com$1
  fi
}

[[ -f /tmp/shelldone.releases.json ]] || ghapi /repos/shelldone/shelldone/releases > /tmp/shelldone.releases.json
[[ -f /tmp/shelldone.nightly.json ]] || ghapi /repos/shelldone/shelldone/releases/tags/nightly > /tmp/shelldone.nightly.json
python3 ci/subst-release-info.py || exit 1
python3 ci/generate-docs.py || exit 1

# Adjust path to pick up pip-installed binaries
PATH="$HOME/.local/bin;$PATH"

if hash black 2>/dev/null ; then
  black ci/generate-docs.py ci/subst-release-info.py
fi

cp "assets/icon/terminal.png" docs/favicon.png
cp "assets/icon/shelldone-icon.svg" docs/favicon.svg
mkdir -p docs/fonts
cp assets/fonts/SymbolsNerdFontMono-Regular.ttf docs/fonts/

docker_or_podman() {
  if hash podman 2>/dev/null ; then
    podman "$@"
  elif hash docker 2>/dev/null ; then
    docker "$@"
  else
    echo "Please install either podman or docker"
    exit 1
  fi
}

docker_or_podman build -t shelldone/mkdocs-material -f ci/Dockerfile.docs .

if [ "$SERVE" == "yes" ] ; then
  docker_or_podman run --rm -it -p8000:8000 -v ${PWD}:/docs shelldone/mkdocs-material serve -a 0.0.0.0:8000
  #docker_or_podman run --rm -it --network=host -v ${PWD}:/docs shelldone/mkdocs-material $@
else
  docker_or_podman run --rm -e CARDS=true -v ${PWD}:/docs shelldone/mkdocs-material build
fi
