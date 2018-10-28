#! /bin/bash
set -e
set -o pipefail

sha256sum target/release/modman
binary_checksum="$(sha256sum target/release/modman | awk '{print $1}')"
echo "Binary Checksum: $binary_checksum"

sha256sum LICENSE
license_checksum="$(sha256sum LICENSE | awk '{print $1}')"
echo "License Checksum: $license_checksum"

sed -e "s/\\(pkgver=\\)/\\1$TRAVIS_TAG/" -e "s/\\(sha256sums=\\)/\\1(\\n    '$binary_checksum'\\n    '$license_checksum'\\n)/" "arch/PKGBUILD.proto" > PKGBUILD
