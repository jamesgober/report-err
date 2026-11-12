#!/usr/bin/env bash
# Fails when a build graph holds two define-err versions. report-err
# re-exports define-err's types and reads its reporter and creation-hook
# slots, and each define-err version has its own slots: with two in one
# build, reports and the hook silently go to a copy the application never
# set up (D-076). Checks this repository's packages, then an application
# that asks for the newest define-err beside report-err, so a new define-err
# line fails here until report-err follows it.
set -euo pipefail

# `pwd -W` gives a path Cargo reads under Git Bash on Windows.
root="$(cd "$(dirname "$0")/.." && (pwd -W 2>/dev/null || pwd))"
# The application's define-err requirement; another value tests the check.
want="${DEFINE_ERR_REQ:-*}"

check() {
    local out
    out="$(cargo tree --manifest-path "$1" -d --target all -e normal,build,dev)"
    if grep -q '^define-err v' <<<"$out"; then
        printf '%s\n' "$out"
        echo "error: two define-err versions in the graph of $1" >&2
        exit 1
    fi
    echo "one define-err: $1"
}

for manifest in Cargo.toml benches/Cargo.toml fuzz/Cargo.toml soak/Cargo.toml; do
    check "$root/$manifest"
done

app="$(mktemp -d)"
: "${app:?}"
mkdir -p "$app/src"
echo 'fn main() {}' > "$app/src/main.rs"
cat > "$app/Cargo.toml" <<TOML
[package]
name = "one-define-err-app"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
report-err = { path = "$root" }
define-err = "$want"

[workspace]
TOML
check "$app/Cargo.toml"
rm -rf "${app:?}"
