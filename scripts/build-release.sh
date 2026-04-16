#!/usr/bin/env bash
# =============================================================================
# build-release.sh — Genera los instaladores de AlcaparraLang
#
# Uso:
#   ./scripts/build-release.sh [--version 0.1.0] [--lang-repo ../alcaparra-lang]
#
# Estrategia multi-máquina:
#   Mac Intel (x86_64) → produce macos-x86_64 + linux-x86_64
#   Mac Apple Silicon (arm64) → produce macos-arm64
#   Los artefactos de ambas máquinas se combinan en un solo GitHub Release.
#
# Produce en ./dist/ (según host):
#   [arm64]   alcaparra-macos-arm64.pkg  alcaparra-macos-arm64.tar.gz
#   [x86_64]  alcaparra-macos-x86_64.pkg alcaparra-linux-x86_64.deb
#             alcaparra-linux-x86_64.tar.gz alcaparra-macos-x86_64.tar.gz
#   [ambos]   install.sh
#
# Requisitos en Mac Intel:
#   rustup target add x86_64-unknown-linux-gnu
#   cargo install cargo-zigbuild
#   brew install zig dpkg
#
# Requisitos en Mac Apple Silicon:
#   rustup target add aarch64-apple-darwin   (normalmente ya instalado)
#   brew install dpkg                        (solo para install.sh)
# =============================================================================

set -euo pipefail

# ── Colores ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'; GREEN='\033[0;32m'; YELLOW='\033[1;33m'
CYAN='\033[0;36m'; BOLD='\033[1m'; NC='\033[0m'

info()    { echo -e "${CYAN}→${NC} $*"; }
success() { echo -e "${GREEN}✓${NC} $*"; }
warn()    { echo -e "${YELLOW}⚠${NC} $*"; }
die()     { echo -e "${RED}✗${NC} $*" >&2; exit 1; }

# ── Argumentos ───────────────────────────────────────────────────────────────
VERSION="0.1.0"
LSP_REPO="$(cd "$(dirname "$0")/.." && pwd)"   # directorio de este repo
LANG_REPO="$(cd "$LSP_REPO/../alcaparra-lang" && pwd 2>/dev/null || echo "")"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --version)   VERSION="$2";   shift 2 ;;
    --lang-repo) LANG_REPO="$2"; shift 2 ;;
    *) die "Argumento desconocido: $1" ;;
  esac
done

[[ -z "$LANG_REPO" || ! -d "$LANG_REPO" ]] && \
  die "No se encontró alcaparra-lang. Usa --lang-repo <ruta>"

DIST="$LSP_REPO/dist"
STAGING="$DIST/staging"

# ── Targets según arquitectura del host ──────────────────────────────────────
HOST_ARCH="$(uname -m)"

if [[ "$HOST_ARCH" == "arm64" ]]; then
  # Mac Apple Silicon — solo compilación nativa arm64
  TARGETS=(
    "aarch64-apple-darwin:macos-arm64"
  )
  info "Host Apple Silicon detectado — compilando solo macos-arm64"
else
  # Mac Intel (o Linux x86_64) — compilación nativa x86_64 + cross-compile Linux
  TARGETS=(
    "x86_64-apple-darwin:macos-x86_64"
    "x86_64-unknown-linux-gnu:linux-x86_64"
  )
  info "Host Intel x86_64 detectado — compilando macos-x86_64 + linux-x86_64"
fi

# ── Verificar dependencias ────────────────────────────────────────────────────
check_deps() {
  info "Verificando dependencias..."
  local missing=()
  command -v cargo    &>/dev/null || missing+=("cargo")
  command -v pkgbuild &>/dev/null || missing+=("pkgbuild  →  xcode-select --install")

  # zigbuild y zig solo son necesarios en Intel (cross-compile Linux)
  if [[ "$HOST_ARCH" != "arm64" ]]; then
    command -v cargo-zigbuild &>/dev/null || missing+=("cargo-zigbuild  →  cargo install cargo-zigbuild")
    command -v zig            &>/dev/null || missing+=("zig              →  brew install zig")
    command -v dpkg-deb       &>/dev/null || missing+=("dpkg-deb         →  brew install dpkg")
  fi

  for tgt_pair in "${TARGETS[@]}"; do
    local tgt="${tgt_pair%%:*}"
    rustup target list --installed 2>/dev/null | grep -q "^$tgt$" || \
      missing+=("rustup target: $tgt  →  rustup target add $tgt")
  done

  if [[ ${#missing[@]} -gt 0 ]]; then
    die "Faltan dependencias:\n$(printf '  %s\n' "${missing[@]}")"
  fi
  success "Dependencias OK"
}

# ── Compilar un binario para un target ────────────────────────────────────────
build_binary() {
  local repo="$1" bin_name="$2" target="$3" platform="$4"
  local out_dir="$STAGING/$platform/usr/local/bin"
  mkdir -p "$out_dir"

  info "[$platform] Compilando $bin_name..."
  pushd "$repo" > /dev/null

  local is_native=false
  [[ "$target" == "x86_64-apple-darwin" && "$HOST_ARCH" == "x86_64" ]] && is_native=true
  [[ "$target" == "aarch64-apple-darwin" && "$HOST_ARCH" == "arm64"  ]] && is_native=true

  if $is_native; then
    # Compilación nativa — cargo build estándar
    cargo build --release --target "$target" --quiet
  else
    # Cross-compile vía zigbuild (ej. Linux desde Mac Intel)
    cargo zigbuild --release --target "$target" --quiet
  fi

  cp "target/$target/release/$bin_name" "$out_dir/$bin_name"
  chmod +x "$out_dir/$bin_name"
  popd > /dev/null
  success "[$platform] $bin_name listo"
}

# ── Crear .pkg para macOS ─────────────────────────────────────────────────────
make_pkg() {
  local platform="$1"   # macos-arm64 | macos-x86_64
  local staging_dir="$STAGING/$platform"
  local pkg_out="$DIST/alcaparra-$platform.pkg"

  info "[$platform] Creando .pkg..."

  # Scripts de post-install (opcional: mensaje de bienvenida)
  local scripts_dir="$STAGING/${platform}-scripts"
  mkdir -p "$scripts_dir"
  cat > "$scripts_dir/postinstall" <<'SCRIPT'
#!/bin/sh
echo ""
echo "  AlcaparraLang instalado correctamente."
echo "  Ejecuta: caper --version"
echo ""
SCRIPT
  chmod +x "$scripts_dir/postinstall"

  pkgbuild \
    --root        "$staging_dir" \
    --scripts     "$scripts_dir" \
    --identifier  "cl.mongoosestudio.alcaparra" \
    --version     "$VERSION" \
    --install-location "/" \
    "$pkg_out"

  success "[$platform] → $(basename "$pkg_out")"
}

# ── Crear .deb para Linux ─────────────────────────────────────────────────────
make_deb() {
  local platform="linux-x86_64"
  local pkg_name="alcaparra_${VERSION}_amd64"
  local pkg_dir="$STAGING/$pkg_name"

  info "[$platform] Creando .deb..."
  mkdir -p "$pkg_dir/DEBIAN"
  # Copiar binarios ya compilados al layout del deb
  mkdir -p "$pkg_dir/usr/local/bin"
  cp "$STAGING/$platform/usr/local/bin/caper"          "$pkg_dir/usr/local/bin/"
  cp "$STAGING/$platform/usr/local/bin/alcaparra-lsp"  "$pkg_dir/usr/local/bin/"

  cat > "$pkg_dir/DEBIAN/control" <<CONTROL
Package: alcaparra
Version: $VERSION
Architecture: amd64
Maintainer: Marcel Rojas <contact@mongoosestudio.cl>
Description: AlcaparraLang — runtime y servidor LSP
 AlcaparraLang es un lenguaje de scripting determinista para lógica de negocio.
 Incluye el runtime caper y el servidor LSP alcaparra-lsp.
Homepage: https://alcaparra.mongoosestudio.cl
CONTROL

  cat > "$pkg_dir/DEBIAN/postinst" <<'POSTINST'
#!/bin/sh
echo ""
echo "  AlcaparraLang instalado correctamente."
echo "  Ejecuta: caper --version"
echo ""
POSTINST
  chmod 755 "$pkg_dir/DEBIAN/postinst"

  dpkg-deb --build "$pkg_dir" "$DIST/alcaparra-$platform.deb"
  success "[$platform] → alcaparra-$platform.deb"
}

# ── Crear .tar.gz ─────────────────────────────────────────────────────────────
make_targz() {
  local platform="$1"
  info "[$platform] Creando .tar.gz..."

  tar -czf "$DIST/alcaparra-$platform.tar.gz" \
    -C "$STAGING/$platform/usr/local/bin" \
    caper alcaparra-lsp

  success "[$platform] → alcaparra-$platform.tar.gz"
}

# ── Generar install.sh ────────────────────────────────────────────────────────
make_install_sh() {
  info "Generando install.sh..."
  local GH_BASE="https://github.com/mongoose-studio/alcaparra-lsp/releases/download/v${VERSION}"

  cat > "$DIST/install.sh" <<INSTALLSH
#!/bin/sh
# AlcaparraLang installer — v${VERSION}
# Uso: curl -fsSL https://alcaparra.mongoosestudio.cl/install.sh | sh
set -e

VERSION="${VERSION}"
BASE="${GH_BASE}"
INSTALL_DIR="\${ALCAPARRA_INSTALL_DIR:-/usr/local/bin}"

RED='\033[0;31m'; GREEN='\033[0;32m'; CYAN='\033[0;36m'; NC='\033[0m'
info()    { printf "\${CYAN}→\${NC} %s\n" "\$*"; }
success() { printf "\${GREEN}✓\${NC} %s\n" "\$*"; }
die()     { printf "\${RED}✗\${NC} %s\n" "\$*" >&2; exit 1; }

# Detectar plataforma
OS=\$(uname -s); ARCH=\$(uname -m)
case "\${OS}-\${ARCH}" in
  Darwin-arm64)   PLATFORM="macos-arm64"  ;;
  Darwin-x86_64)  PLATFORM="macos-x86_64" ;;
  Linux-x86_64)   PLATFORM="linux-x86_64" ;;
  *) die "Plataforma no soportada: \${OS}-\${ARCH}" ;;
esac

info "Instalando AlcaparraLang v\${VERSION} para \${PLATFORM}..."

# Descargar
TMP=\$(mktemp -d)
trap 'rm -rf "\$TMP"' EXIT

info "Descargando caper..."
curl -fsSL "\${BASE}/caper-\${PLATFORM}" -o "\$TMP/caper"

info "Descargando alcaparra-lsp..."
curl -fsSL "\${BASE}/alcaparra-lsp-\${PLATFORM}" -o "\$TMP/alcaparra-lsp"

chmod +x "\$TMP/caper" "\$TMP/alcaparra-lsp"

# Instalar (con sudo si no tenemos permisos)
if [ -w "\$INSTALL_DIR" ]; then
  mv "\$TMP/caper"          "\$INSTALL_DIR/caper"
  mv "\$TMP/alcaparra-lsp"  "\$INSTALL_DIR/alcaparra-lsp"
else
  info "Se requieren permisos de administrador para instalar en \$INSTALL_DIR"
  sudo mv "\$TMP/caper"          "\$INSTALL_DIR/caper"
  sudo mv "\$TMP/alcaparra-lsp"  "\$INSTALL_DIR/alcaparra-lsp"
fi

success "AlcaparraLang v\${VERSION} instalado en \${INSTALL_DIR}"
echo ""
echo "  Prueba con:  caper --version"
echo "  Docs:        https://alcaparra.mongoosestudio.cl"
echo ""
INSTALLSH

  chmod +x "$DIST/install.sh"
  success "install.sh listo"
}

# ── Main ──────────────────────────────────────────────────────────────────────
main() {
  echo -e "\n${BOLD}AlcaparraLang Release Builder v${VERSION}${NC}\n"

  check_deps

  rm -rf "$DIST"
  mkdir -p "$DIST" "$STAGING"

  # Compilar binarios para cada plataforma
  for tgt_pair in "${TARGETS[@]}"; do
    local target="${tgt_pair%%:*}"
    local platform="${tgt_pair##*:}"
    build_binary "$LANG_REPO" "caper"          "$target" "$platform"
    build_binary "$LSP_REPO"  "alcaparra-lsp"  "$target" "$platform"
  done

  # Empaquetar solo las plataformas compiladas en este host
  for tgt_pair in "${TARGETS[@]}"; do
    local platform="${tgt_pair##*:}"
    case "$platform" in
      macos-arm64|macos-x86_64)
        make_pkg    "$platform"
        make_targz  "$platform"
        ;;
      linux-x86_64)
        make_deb
        make_targz  "$platform"
        ;;
    esac
  done

  make_install_sh

  echo -e "\n${BOLD}${GREEN}¡Listo!${NC} Artefactos generados en ${BOLD}./dist/${NC}:\n"
  ls -lh "$DIST" | grep -v "^total" | awk '{print "  " $NF "\t" $5}'
  echo ""

  if [[ "$HOST_ARCH" == "arm64" ]]; then
    warn "Solo se compiló macos-arm64. Para completar el release:"
    echo -e "  1. Ejecuta este script en el Mac Intel (x86_64)"
    echo -e "  2. Copia los artefactos de ambas máquinas en una carpeta dist/ común"
    echo -e "  3. Luego:"
  else
    warn "Solo se compiló macos-x86_64 + linux-x86_64. Para completar el release:"
    echo -e "  1. Ejecuta este script en el Mac Apple Silicon (arm64)"
    echo -e "  2. Copia los artefactos de ambas máquinas en una carpeta dist/ común"
    echo -e "  3. Luego:"
  fi
  echo -e "  ${CYAN}gh release create v${VERSION} dist/* --title 'v${VERSION}' --notes 'Early Access'${NC}"
  echo ""
}

main "$@"
