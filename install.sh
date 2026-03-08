#!/bin/bash
# install.sh — Install Rui (after building)
#
# Usage:
#   ./build.sh          # build first
#   ./install.sh        # then install
#
# Or just run build.sh which does both.
#
# Installs:
#   - Binary -> /usr/local/bin/rui
#   - Icon   -> /usr/share/icons/hicolor/scalable/apps/org.rui.designer.svg
#   - Desktop entry -> /usr/share/applications/org.rui.designer.desktop
#
# Uninstall with: ./install.sh --uninstall

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

# ─── Uninstall ──────────────────────────────────────────────────
if [[ "${1:-}" == "--uninstall" ]]; then
    echo -e "${BOLD}  Uninstalling Rui...${NC}"
    sudo rm -f "$PREFIX/bin/rui"
    sudo rm -f /usr/share/applications/org.rui.designer.desktop
    sudo rm -f /usr/share/icons/hicolor/scalable/apps/org.rui.designer.svg
    sudo gtk-update-icon-cache -f /usr/share/icons/hicolor/ 2>/dev/null || true
    echo -e "${GREEN}${BOLD}  ✓ Rui uninstalled.${NC}"
    exit 0
fi

# ─── Check binary exists ───────────────────────────────────────
BINARY="$SCRIPT_DIR/target/release/rui"
if [[ ! -f "$BINARY" ]]; then
    err "Binary not found at $BINARY — run ./build.sh first."
fi

echo -e "${BOLD}"
echo "  ╔══════════════════════════════════════╗"
echo "  ║     Rui — Install                    ║"
echo "  ║     GTK4 UI Designer for Rust        ║"
echo "  ╚══════════════════════════════════════╝"
echo -e "${NC}"

# ─── Install binary ─────────────────────────────────────────────
info "Installing binary -> $PREFIX/bin/rui"
sudo install -Dm755 "$BINARY" "$PREFIX/bin/rui"
ok "$PREFIX/bin/rui"

# ─── Install icon ───────────────────────────────────────────────
ICON_SRC="$SCRIPT_DIR/data/org.rui.designer.svg"
ICON_DST="/usr/share/icons/hicolor/scalable/apps/org.rui.designer.svg"

info "Installing icon -> $ICON_DST"
sudo install -Dm644 "$ICON_SRC" "$ICON_DST"
ok "$ICON_DST"

# Update icon cache so the icon shows immediately
sudo gtk-update-icon-cache -f /usr/share/icons/hicolor/ 2>/dev/null || true

# ─── Install desktop entry ──────────────────────────────────────
DESKTOP_SRC="$SCRIPT_DIR/data/org.rui.designer.desktop"
DESKTOP_DST="/usr/share/applications/org.rui.designer.desktop"

info "Installing desktop entry -> $DESKTOP_DST"
sudo install -Dm644 "$DESKTOP_SRC" "$DESKTOP_DST"
ok "$DESKTOP_DST"

# Update desktop database
sudo update-desktop-database /usr/share/applications/ 2>/dev/null || true

echo ""
echo -e "${GREEN}${BOLD}  ✓ Rui installed successfully.${NC}"
echo ""
echo "  Run:  rui"
echo "  Or open .ui files with Rui from your file manager."
echo "  Uninstall:  ./install.sh --uninstall"
echo ""
