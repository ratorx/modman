#! /bin/bash
set -e
set -o pipefail

binary_checksum="$(sha256sum -sd target/release/modman | awk '{print $1}')"
license_checksum="$(sha256sum LICENSE | awk '{print $1}')"

sed -e "s/\\(pkgver=\\)/\\1$TRAVIS_TAG/" -e "s/\\(sha256sums=\\)/\\1(\\n    '$binary_checksum'\\n    '$license_checksum'\\n)/" "arch/PKGBUILD.proto" > PKGBUILD
