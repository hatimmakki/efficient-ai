#!/usr/bin/env sh
# 1-command installer for efficient-ai (`eai`) on macOS & Linux.
# Usage: curl -fsSL https://raw.githubusercontent.com/hatimmakki/efficient-ai/master/install.sh | sh
set -eu

REPO="hatimmakki/efficient-ai"
INSTALL_DIR="${EAI_INSTALL_DIR:-$HOME/.eai/bin}"
VERSION="${EAI_VERSION:-latest}"

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      arm64|aarch64) TARGET="aarch64-apple-darwin" ;;
      x86_64)        TARGET="x86_64-apple-darwin" ;;
      *) echo "Unsupported macOS architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64|amd64) TARGET="x86_64-unknown-linux-gnu" ;;
      *) echo "Unsupported Linux architecture for prebuilt binary: $ARCH (fallback: cargo install --git https://github.com/$REPO eai)" >&2; exit 1 ;;
    esac
    ;;
  *)
    echo "Unsupported OS: $OS. On Windows, run install.ps1." >&2
    exit 1
    ;;
esac

mkdir -p "$INSTALL_DIR"

if [ "$VERSION" = "latest" ]; then
  BASE_URL="https://github.com/$REPO/releases/latest/download"
else
  BASE_URL="https://github.com/$REPO/releases/download/$VERSION"
fi

ARCHIVE="eai-${TARGET}.tar.gz"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

echo "Downloading efficient-ai ($TARGET)..."
if curl -fsSL "$BASE_URL/$ARCHIVE" -o "$TMP_DIR/$ARCHIVE"; then
  tar -xzf "$TMP_DIR/$ARCHIVE" -C "$TMP_DIR"
  mv "$TMP_DIR/eai" "$INSTALL_DIR/eai"
  chmod +x "$INSTALL_DIR/eai"
elif command -v cargo >/dev/null 2>&1; then
  echo "Prebuilt release not found yet; building from source via cargo..."
  cargo install --git "https://github.com/$REPO.git" eai --root "$HOME/.eai"
else
  echo "Failed to download $BASE_URL/$ARCHIVE and cargo is not installed." >&2
  exit 1
fi

# Create convenience wrappers for `outline` and `squeeze`
cat > "$INSTALL_DIR/outline" <<EOF
#!/usr/bin/env sh
exec "$INSTALL_DIR/eai" outline "\$@"
EOF
cat > "$INSTALL_DIR/squeeze" <<EOF
#!/usr/bin/env sh
exec "$INSTALL_DIR/eai" squeeze "\$@"
EOF
chmod +x "$INSTALL_DIR/outline" "$INSTALL_DIR/squeeze"

echo ""
echo "Installed eai to $INSTALL_DIR/eai"
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo "Add to PATH:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac
echo ""
echo "Next steps:"
echo "  eai analyze          # See where your AI tokens & dollars go across all agents"
echo "  eai init             # 1-command auto-setup for Claude Code, Antigravity, Gemini CLI & Codex"
