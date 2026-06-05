#!/bin/sh
# Taipan! installer — downloads the latest prebuilt binary for your platform.
#
#   curl -fsSL https://raw.githubusercontent.com/turlockmike/taipan-rs/main/install.sh | sh
#
# Honors:
#   TAIPAN_INSTALL_DIR  install location (default: $HOME/.local/bin)
#   TAIPAN_VERSION      tag to install (default: latest release)
set -eu

REPO="turlockmike/taipan-rs"
BIN="taipan"
INSTALL_DIR="${TAIPAN_INSTALL_DIR:-$HOME/.local/bin}"

err() {
	echo "install: $*" >&2
	exit 1
}

# --- detect platform -> Rust target triple ---------------------------------
os="$(uname -s)"
arch="$(uname -m)"
case "$os" in
Darwin)
	case "$arch" in
	arm64 | aarch64) target="aarch64-apple-darwin" ;;
	x86_64) target="x86_64-apple-darwin" ;;
	*) err "unsupported macOS arch: $arch" ;;
	esac
	;;
Linux)
	case "$arch" in
	x86_64) target="x86_64-unknown-linux-gnu" ;;
	*) err "unsupported Linux arch: $arch (try: cargo install --git https://github.com/$REPO)" ;;
	esac
	;;
*)
	err "unsupported OS: $os (try: cargo install --git https://github.com/$REPO)"
	;;
esac

# --- resolve version --------------------------------------------------------
version="${TAIPAN_VERSION:-latest}"
if [ "$version" = "latest" ]; then
	base="https://github.com/$REPO/releases/latest/download"
else
	base="https://github.com/$REPO/releases/download/$version"
fi

tarball="${BIN}-${target}.tar.gz"
url="$base/$tarball"

# --- download into a temp dir ----------------------------------------------
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

echo "Downloading $tarball ..."
if command -v curl >/dev/null 2>&1; then
	curl -fSL "$url" -o "$tmp/$tarball" || err "download failed: $url"
	curl -fSL "$url.sha256" -o "$tmp/$tarball.sha256" 2>/dev/null || true
elif command -v wget >/dev/null 2>&1; then
	wget -qO "$tmp/$tarball" "$url" || err "download failed: $url"
	wget -qO "$tmp/$tarball.sha256" "$url.sha256" 2>/dev/null || true
else
	err "need curl or wget"
fi

# --- verify checksum if we got one -----------------------------------------
if [ -s "$tmp/$tarball.sha256" ]; then
	expected="$(awk '{print $1}' "$tmp/$tarball.sha256")"
	if command -v sha256sum >/dev/null 2>&1; then
		actual="$(sha256sum "$tmp/$tarball" | awk '{print $1}')"
	else
		actual="$(shasum -a 256 "$tmp/$tarball" | awk '{print $1}')"
	fi
	[ "$expected" = "$actual" ] || err "checksum mismatch (expected $expected, got $actual)"
	echo "Checksum verified."
fi

# --- install ---------------------------------------------------------------
tar -xzf "$tmp/$tarball" -C "$tmp"
mkdir -p "$INSTALL_DIR"
mv "$tmp/$BIN" "$INSTALL_DIR/$BIN"
chmod +x "$INSTALL_DIR/$BIN"

echo "Installed $BIN to $INSTALL_DIR/$BIN"
case ":$PATH:" in
*":$INSTALL_DIR:"*) ;;
*) echo "Note: $INSTALL_DIR is not on your PATH. Add it, e.g.:
  export PATH=\"$INSTALL_DIR:\$PATH\"" ;;
esac
echo "Run '$BIN --help' to get started."
