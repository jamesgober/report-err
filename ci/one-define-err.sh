#!/usr/bin/env bash
# One define-err per build (D-076). report-err re-exports define-err's types
# and reads its reporter and creation-hook slots, and each define-err version
# has its own slots: with two in one build, reports and the hook silently go
# to a copy the application never set up.
#
# Fails when this repository's packages, or an application on the define-err
# line report-err requires, hold two define-err versions. Warns (without
# failing) when a newer define-err line exists: report-err follows it in its
# next minor release, since moving is a breaking change for its users.
set -euo pipefail

# `pwd -W` gives a path Cargo reads under Git Bash on Windows.
root="$(cd "$(dirname "$0")/.." && (pwd -W 2>/dev/null || pwd))"
# The define-err requirement in report-err's manifest, such as `0.9`.
ours="$(sed -n 's/^define-err = { version = "\([^"]*\)".*/\1/p' "$root/Cargo.toml")"
if [ -z "$ours" ]; then
    echo "error: no define-err requirement found in $root/Cargo.toml" >&2
    exit 1
fi

# Prints the duplicate tree and returns 1 when the graph holds two define-err
# versions.
two_copies() {
    local out
    out="$(cargo tree --manifest-path "$1" -d --target all -e normal,build,dev 2>/dev/null)"
    if grep -q '^define-err v' <<<"$out"; then
        printf '%s\n' "$out"
        return 1
    fi
    return 0
}

app="$(mktemp -d)"
: "${app:?}"
trap 'rm -rf "${app:?}"' EXIT
mkdir -p "$app/src"
echo 'fn main() {}' > "$app/src/main.rs"

# An application that depends on report-err and on define-err `$1`.
make_app() {
    cat > "$app/Cargo.toml" <<TOML
[package]
name = "one-define-err-app"
version = "0.0.0"
edition = "2021"
publish = false

[dependencies]
report-err = { path = "$root" }
define-err = "$1"

[workspace]
TOML
    rm -f "$app/Cargo.lock"
}

failed=0
for manifest in Cargo.toml benches/Cargo.toml fuzz/Cargo.toml soak/Cargo.toml; do
    if two_copies "$root/$manifest"; then
        echo "one define-err: $manifest"
    else
        echo "error: two define-err versions in the graph of $manifest" >&2
        failed=1
    fi
done

make_app "${DEFINE_ERR_REQ:-$ours}"
if two_copies "$app/Cargo.toml"; then
    echo "one define-err: an application on define-err ${DEFINE_ERR_REQ:-$ours}"
else
    echo "error: an application on define-err ${DEFINE_ERR_REQ:-$ours} gets two copies" >&2
    failed=1
fi

make_app "*"
if two_copies "$app/Cargo.toml" >/dev/null; then
    echo "define-err $ours is the newest line"
else
    newest="$(cargo tree --manifest-path "$app/Cargo.toml" -e normal --depth 1 2>/dev/null \
        | sed -n 's/.*define-err v\([0-9.]*\).*/\1/p' | sort -V | tail -1)"
    echo "::warning::define-err ${newest:-a newer line} is out and report-err requires $ours; an application on it gets two copies until report-err's next minor release follows it"
fi

exit "$failed"
