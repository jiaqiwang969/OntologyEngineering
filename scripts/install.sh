#!/bin/sh
# Build the complete, locked workspace entry points in this checkout.
# External professional tools are installed separately by the user.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd -P)
profile=dev
offline=
jobs=2
while [ "$#" -gt 0 ]; do
    case "$1" in
        --release) profile=release ;;
        --offline) offline=--offline ;;
        --jobs) shift; jobs=${1:?--jobs requires a positive integer} ;;
        --help|-h)
            printf '%s\n' 'Usage: sh scripts/install.sh [--release] [--offline] [--jobs N]' \
                'Installs all core programs, including memory, into this checkout/var/install/bin.' \
                'Default: development profile, two compiler jobs; the profile is recorded.' \
                'Requires Git, the pinned Rust toolchain, and a platform C/C++ linker.'
            exit 0 ;;
        *) printf '%s\n' "Unknown argument: $1" >&2; exit 2 ;;
    esac
    shift
done
case "$jobs" in ''|*[!0-9]*|0) printf '%s\n' 'Invalid compiler job count' >&2; exit 2 ;; esac
command -v cargo >/dev/null 2>&1 || { printf '%s\n' 'Install Rust with rustup first: https://rustup.rs/' >&2; exit 1; }
cd "$root"
target="$root/var/builds/rust-target"
if [ "$profile" = dev ]; then output=debug; else output=release; fi
# Keep the target inside this checkout; never pick up a previous skill's binaries.
export CARGO_TARGET_DIR="$target"
cargo build --locked $offline --profile "$profile" --jobs "$jobs" \
    --bin oev2 --bin oev2-design-bootstrap --bin oe-code-runner \
    --bin oe-code-evaluator --bin oe-evolution-driver \
    --bin oe-runtime-linker2 --bin oe-runtime-rustc2 --bin engineering-memory

destination="$root/var/install"
stage=$(mktemp -d "$root/var/install-stage.XXXXXX")
suffix=${stage##*.}
mkdir "$stage/bin"
trap 'rm -rf -- "$stage"' EXIT HUP INT TERM
for program in oev2 oev2-design-bootstrap oe-code-runner oe-code-evaluator oe-evolution-driver oe-runtime-linker2 oe-runtime-rustc2 engineering-memory; do
    install -m 755 "$target/$output/$program" "$stage/bin/$program"
done
(
    cd "$stage"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum bin/* > SHA256SUMS
        sha256sum -c SHA256SUMS
    else
        shasum -a 256 bin/* > SHA256SUMS
        shasum -a 256 -c SHA256SUMS
    fi
    { printf 'profile=%s\n' "$profile"; rustc --version; cargo --version; } > BUILD.txt
)
# Keep the prior complete installation until this one has passed basic startup.
"$stage/bin/oev2" --root "$root" --version
"$stage/bin/oev2" --root "$root" memory capabilities
if [ -e "$destination" ]; then
    previous="$root/var/install-previous.$suffix"
    test ! -e "$previous"
    mv "$destination" "$previous"
fi
mv "$stage" "$destination"
trap - EXIT HUP INT TERM
if ! "$destination/bin/oev2" --root "$root" setup; then
    mv "$destination" "$root/var/install-failed.$suffix"
    if [ -n "${previous:-}" ]; then mv "$previous" "$destination"; fi
    printf '%s\n' 'Installation identity check failed; prior installation restored when present.' >&2
    exit 1
fi
printf '\n%s\n' "Installed: $destination/bin/oev2" \
    "Core memory: $destination/bin/oev2 memory status" \
    'Configure your agent using AGENTS.md; install professional tools using docs/TOOL-INTEGRATIONS.md.'
