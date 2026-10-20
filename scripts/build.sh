#!/usr/bin/env bash

target=$1
platform="x86_64-unknown-linux-musl"

IFS="= "
while read -r name value; do
    if [[ $name == "version" ]]; then
        version=${value//\"/}
    fi
done < Cargo.toml

echo "Compile nur-cms \"$version\""
echo ""

if [[ "$(uname -s)" == "Darwin" ]]; then
    compiler="x86_64-linux-musl-gcc"
    TARGET_CC="$compiler" cargo build --release --target="$platform" \
        --config "target.${platform}.linker=\"${compiler}\"" || exit $?
else
    cargo build --release --target="$platform" || exit $?
fi

if [[ "$target" == "debian" ]]; then
    cargo deb --no-build --target=$platform -p nur-cms --manifest-path=crates/app/Cargo.toml -o nur-cms_${version}-1_amd64.deb
elif [[ "$target" == "rhel" ]]; then
    cargo generate-rpm --target=$platform -p crates/app -o nur-cms-${version}-1.x86_64.rpm
else
    cargo deb --no-build --target=$platform -p nur-cms --manifest-path=crates/app/Cargo.toml -o nur-cms_${version}-1_amd64.deb
    cargo generate-rpm --target=$platform -p crates/app -o nur-cms-${version}-1.x86_64.rpm
fi
