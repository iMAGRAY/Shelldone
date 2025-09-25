#!/bin/bash
set -x
name="$1"

notes=$(cat <<EOT
See https://shelldone.org/changelog.html#$name for the changelog

If you're looking for nightly downloads or more detailed installation instructions:

[Windows](https://shelldone.org/install/windows.html)
[macOS](https://shelldone.org/install/macos.html)
[Linux](https://shelldone.org/install/linux.html)
[FreeBSD](https://shelldone.org/install/freebsd.html)
EOT
)

gh release view "$name" || gh release create --prerelease --notes "$notes" --title "$name" "$name"
