#!/bin/bash
# build.sh — Build and install Rui
# Usage: ./build.sh
# Requires: rust/cargo, sudo access

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
BOLD='\033[1m'
NC='\033[0m'

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PREFIX="${PREFIX:-/usr/local}"

info()  { echo -e "${BLUE}==>${NC} ${BOLD}$1${NC}"; }
ok()    { echo -e "${GREEN}  ✓${NC} $1"; }
err()   { echo -e "${RED}  ✗ $1${NC}"; exit 1; }

echo -e "${BOLD}"
echo "  ╔══════════════════════════════════════╗"
echo "  ║     Rui — Build & Install            ║"
echo "  ║     GTK4 UI Designer for Rust        ║"
echo "  ╚══════════════════════════════════════╝"
echo -e "${NC}"

cd "$SCRIPT_DIR"

# ─── Check prerequisites ───────────────────────────────────────

info "Checking prerequisites..."

missing=()

command -v cargo >/dev/null 2>&1 || missing+=("rust/cargo")

if pkg-config --exists gtk4 2>/dev/null; then
    ok "gtk4"
else
    missing+=("gtk4")
fi

if pkg-config --exists gtksourceview-5 2>/dev/null; then
    ok "gtksourceview5"
else
    missing+=("gtksourceview5")
fi

if pkg-config --exists webkitgtk-6.0 2>/dev/null; then
    ok "webkit2gtk-6.0"
else
    echo -e "  ${BLUE}ℹ${NC} webkit2gtk-6.0 not found — building without preview/AI panel"
fi

if [ ${#missing[@]} -gt 0 ]; then
    err "Missing dependencies: ${missing[*]}"
    echo ""
    echo "  Install on Arch Linux:"
    echo "    sudo pacman -S rust gtk4 gtksourceview5 webkit2gtk-6.0"
    echo ""
    echo "  Then re-run this script."
    exit 1
fi

ok "All prerequisites found"

# ─── Build ──────────────────────────────────────────────────────

# Auto-detect whether to enable preview feature
if pkg-config --exists webkitgtk-6.0 2>/dev/null; then
    info "Building Rui (release, with preview)..."
    cargo build --release
else
    info "Building Rui (release, no preview)..."
    cargo build --release --no-default-features
fi

ok "Build complete"

# ─── Install ────────────────────────────────────────────────────

info "Installing rui -> $PREFIX/bin/rui (requires sudo)..."
sudo install -Dm755 target/release/rui "$PREFIX/bin/rui"
ok "Installed rui"

# ─── Desktop entry ──────────────────────────────────────────────

info "Installing desktop entry..."
DESKTOP_DIR="/usr/share/applications"

sudo tee "$DESKTOP_DIR/rui.desktop" > /dev/null <<EOF
[Desktop Entry]
Name=Rui
Comment=GTK4 UI Designer for Rust
Exec=rui %F
Icon=text-editor
Terminal=false
Type=Application
Categories=Development;GTK;
MimeType=application/x-gtk-builder;application/xml;
EOF

ok "$DESKTOP_DIR/rui.desktop"

echo ""
echo -e "${GREEN}${BOLD}  ✓ Rui installed successfully.${NC}"
echo ""
echo "  Run:  rui"
echo "  Or open .ui files with Rui from your file manager."
echo ""
