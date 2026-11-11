#!/bin/sh
# Installs anycube for the user who runs it: the program, and if wanted
# an entry among the computer's applications. Nothing is written outside
# the user's own directories and nothing needs sudo.
#
#   curl -fsSL https://raw.githubusercontent.com/Horia73/anycube/main/install.sh | sh
#                                from nothing: fetches what it needs itself
#   ./install.sh                 from a clone of the repository
#   ./install.sh --app           also add it to the applications, without asking
#   ./install.sh --no-app        do not, without asking
#   ./install.sh --from-source   build here even if a ready-made program exists
#   ./install.sh --uninstall     remove everything this script installed
#   ./install.sh --uninstall --tables    and the lookup tables too
#
# With arguments down a pipe:  curl ... | sh -s -- --app
#
# It takes a ready-made program from the project's releases when there is
# one for this system. Otherwise it fetches the source (the clone it is run
# from, else a clone or a download of the repository) and builds it.
# Building needs Rust and a C compiler; this script installs neither, it
# says what is missing and how to get it.
set -eu

# The repository on GitHub, as owner/name: where ready-made programs are
# fetched from (its releases) and, without a clone, the source.
REPO="Horia73/anycube"
# Where the source is cloned from, if not from there (a git address or a directory).
SOURCE="${ANYCUBE_SOURCE:-https://github.com/$REPO}"
# How this script was started, for the lines that say how to run it again.
case "$0" in
    *install.sh) piped=no; again="$0" ;;
    *) piped=yes; again="curl -fsSL https://raw.githubusercontent.com/$REPO/main/install.sh | sh -s --" ;;
esac

BIN_DIR="${ANYCUBE_BIN_DIR:-$HOME/.local/bin}"
BIN="$BIN_DIR/anycube"
case "$(uname -s)" in
    Darwin) OS=macos; DATA="$HOME/Library/Application Support/anycube" ;;
    Linux) OS=linux; DATA="${XDG_DATA_HOME:-$HOME/.local/share}/anycube" ;;
    *) OS=other; DATA="$HOME/.anycube" ;;
esac
case "$(uname -m)" in
    arm64 | aarch64) ARCH=arm64 ;;
    x86_64 | amd64) ARCH=x86_64 ;;
    *) ARCH=$(uname -m) ;;
esac

say() { printf '%s\n' "$*"; }
fail() { printf 'error: %s\n' "$*" >&2; exit 1; }

app=ask
source_only=no
uninstall=no
tables=no
for arg in "$@"; do
    case "$arg" in
        --app) app=yes ;;
        --no-app) app=no ;;
        --from-source) source_only=yes ;;
        --uninstall) uninstall=yes ;;
        --tables) tables=yes ;;
        -h | --help)
            if [ "$piped" = no ]; then sed -n '2,21p' "$0" | sed 's/^# \{0,1\}//'; else say "arguments: --app --no-app --from-source --uninstall [--tables]"; fi
            exit 0 ;;
        *) fail "unknown argument '$arg' (see --help)" ;;
    esac
done

if [ "$uninstall" = yes ]; then
    say "Removing anycube:"
    if [ -x "$BIN" ]; then
        "$BIN" app --uninstall | sed 's/^/  /'
        rm -f "$BIN"
        say "  removed $BIN"
    else
        say "  no program at $BIN"
    fi
    # What the interface keeps beside the tables: the profile of its window, where a browser shows it.
    if [ -d "$DATA/window-profile" ]; then
        rm -rf "$DATA/window-profile"
        say "  removed $DATA/window-profile"
    fi
    if [ -d "$DATA/tables" ]; then
        if [ "$tables" = yes ]; then
            rm -rf "$DATA/tables"
            say "  removed $DATA/tables"
        else
            say "  kept the lookup tables in $DATA/tables ($(du -sh "$DATA/tables" | cut -f1 | tr -d ' ')): --uninstall --tables removes them too"
        fi
    fi
    rmdir "$DATA" 2>/dev/null && say "  removed $DATA" || true
    say "Done."
    exit 0
fi

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
here=$(cd "$(dirname "$0")" 2>/dev/null && pwd || pwd)
got=""

# A ready-made program from the releases, if there is one for this system.
if [ "$source_only" = no ] && command -v curl >/dev/null 2>&1; then
    asset="anycube-$OS-$ARCH.tar.gz"
    if curl -fsSL "https://github.com/$REPO/releases/latest/download/$asset" -o "$work/$asset" 2>/dev/null \
        && tar -xzf "$work/$asset" -C "$work" 2>/dev/null && [ -f "$work/anycube" ]; then
        got="$work/anycube"
        say "Fetched the ready-made program for $OS on $ARCH."
    else
        say "No ready-made program for $OS on $ARCH in the releases: building from source."
    fi
fi

# Otherwise from source: the clone this script is in, or a fresh one.
if [ -z "$got" ]; then
    # Rust's installer puts cargo here; a terminal opened before it ran does not know yet.
    [ -x "$HOME/.cargo/bin/cargo" ] && PATH="$HOME/.cargo/bin:$PATH"
    missing=no
    if ! command -v cargo >/dev/null 2>&1; then
        missing=yes
        say "anycube has to be built from source here, and that needs Rust, which is not installed."
        say "Rust's own installer is one command and needs no administrator rights:"
        say "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    fi
    if ! command -v cc >/dev/null 2>&1; then
        missing=yes
        say "Building also needs a C compiler, which is not installed."
        if [ "$OS" = macos ]; then
            say "  xcode-select --install"
        else
            say "  it comes with your system's build tools: the package build-essential, base-devel or gcc"
        fi
    fi
    if [ "$missing" = yes ]; then
        say "Then run this again:"
        say "  $again"
        exit 1
    fi
    if [ "$piped" = no ] && [ -f "$here/Cargo.toml" ] && grep -q '^name = "anycube"' "$here/Cargo.toml"; then
        src="$here"
    elif command -v git >/dev/null 2>&1; then
        say "Fetching the source..."
        git clone --quiet --depth 1 "$SOURCE" "$work/src" || fail "could not clone $SOURCE"
        src="$work/src"
    elif command -v curl >/dev/null 2>&1 && command -v tar >/dev/null 2>&1; then
        # No git: the same source as an archive.
        say "Fetching the source..."
        mkdir "$work/src"
        curl -fsSL "https://github.com/$REPO/archive/refs/heads/main.tar.gz" | tar -xz -C "$work/src" --strip-components 1 \
            || fail "could not download the source from https://github.com/$REPO"
        src="$work/src"
    else
        fail "fetching the source needs git, or curl and tar; none of them is installed"
    fi
    say "Building (a minute or two)..."
    (cd "$src" && cargo build --release --quiet) || fail "the build failed; what cargo said is above"
    got="$src/target/release/anycube"
    [ -f "$got" ] || fail "the build did not produce the program"
fi

mkdir -p "$BIN_DIR"
# A copy, then a move: a program that is running is not written over in place.
cp "$got" "$BIN.new"
chmod 755 "$BIN.new"
mv -f "$BIN.new" "$BIN"
say "Installed:"
say "  $BIN"
case ":$PATH:" in
    *":$BIN_DIR:"*) ;;
    *) say "  ($BIN_DIR is not on your PATH: add it, or start the program by that full path)" ;;
esac

if [ "$OS" = other ]; then
    app=no
    say "An entry among the applications is made on macOS and Linux only, for now."
fi
if [ "$app" = ask ]; then
    # Asked on the terminal even when the script itself comes down a pipe.
    if [ -r /dev/tty ] && [ -w /dev/tty ] && (exec </dev/tty) 2>/dev/null; then
        printf 'Add anycube to your applications, with its icon? [Y/n] ' >/dev/tty
        read -r answer </dev/tty || answer=n
        case "$answer" in n* | N*) app=no ;; *) app=yes ;; esac
    else
        app=no
        say "To add it to your applications later: anycube app --install"
    fi
fi
if [ "$app" = yes ]; then
    "$BIN" app --install | sed 's/^/  /'
fi

say ""
say "Run it:        anycube app"
say "Lookup tables: built on first use, in $DATA/tables"
say "Remove it:     $again --uninstall"
say "               (or: anycube app --uninstall for the entry among the applications alone)"
