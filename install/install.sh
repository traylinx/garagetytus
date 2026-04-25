#!/usr/bin/env bash
# garagetytus installer for macOS and Linux
#
# Usage:
#   curl -fsSL --proto '=https' --tlsv1.2 \
#     https://raw.githubusercontent.com/traylinx/garagetytus/main/install/install.sh | bash
#
# Future canonical URL (post cargo-dist artifacts at v0.1.0 non-rc):
#   curl -fsSL https://garagetytus.dev/install.sh | bash
#
# Env vars (all optional):
#   GARAGETYTUS_NO_PROMPT=1       — non-interactive (skip first-run wizard)
#   GARAGETYTUS_NO_ONBOARD=1      — skip the install + start + bootstrap chain
#   GARAGETYTUS_DRY_RUN=1         — print plan + exit, no side effects
#   GARAGETYTUS_REF=<git-ref>     — branch/tag to install from (default: main)
#   GARAGETYTUS_GUM_VERSION=0.17.0
#   NO_COLOR=1                    — disable ANSI + gum
#
# This installer is the v0.1.0-rc2 source-build path. It compiles
# garagetytus from a fresh git checkout via `cargo install --git`.
# Expect ~3-5 min on first run; subsequent re-runs hit the cargo
# cache and finish in seconds.

set -euo pipefail

# ──────────────────────────────────────────────────────────────
# Color palette (RGB ANSI). Disable on NO_COLOR / non-TTY.
# ──────────────────────────────────────────────────────────────
if [[ -z "${NO_COLOR:-}" && -t 1 ]]; then
    BOLD='\033[1m'
    ACCENT='\033[38;2;217;119;6m'         # amber-600 #d97706
    ACCENT_BRIGHT='\033[38;2;245;158;11m' # amber-500
    INFO='\033[38;2;136;146;176m'
    SUCCESS='\033[38;2;34;197;94m'
    WARN='\033[38;2;255;176;32m'
    ERROR='\033[38;2;230;57;70m'
    MUTED='\033[38;2;90;100;128m'
    NC='\033[0m'
else
    BOLD=''; ACCENT=''; ACCENT_BRIGHT=''; INFO=''; SUCCESS=''
    WARN=''; ERROR=''; MUTED=''; NC=''
fi

# ──────────────────────────────────────────────────────────────
# Temp-file management with EXIT trap cleanup.
# ──────────────────────────────────────────────────────────────
TMPFILES=()
cleanup_tmpfiles() {
    local f
    for f in "${TMPFILES[@]:-}"; do
        rm -rf "$f" 2>/dev/null || true
    done
}
trap cleanup_tmpfiles EXIT

mktempfile() {
    local f
    f="$(mktemp)"
    TMPFILES+=("$f")
    echo "$f"
}

mktempdir() {
    local d
    d="$(mktemp -d)"
    TMPFILES+=("$d")
    echo "$d"
}

# ──────────────────────────────────────────────────────────────
# Downloader detection (curl preferred, wget fallback).
# ──────────────────────────────────────────────────────────────
DOWNLOADER=""
detect_downloader() {
    if command -v curl >/dev/null 2>&1; then
        DOWNLOADER="curl"
        return 0
    fi
    if command -v wget >/dev/null 2>&1; then
        DOWNLOADER="wget"
        return 0
    fi
    echo "garagetytus installer: missing downloader (curl or wget required)" >&2
    exit 1
}

download_file() {
    local url="$1"
    local out="$2"
    if [[ -z "$DOWNLOADER" ]]; then detect_downloader; fi
    if [[ "$DOWNLOADER" == "curl" ]]; then
        curl -fsSL --proto '=https' --tlsv1.2 \
            --retry 3 --retry-delay 1 --retry-connrefused \
            -o "$out" "$url"
        return
    fi
    wget -q --https-only --secure-protocol=TLSv1_2 \
        --tries=3 --timeout=20 -O "$out" "$url"
}

# ──────────────────────────────────────────────────────────────
# OS + arch detection.
# ──────────────────────────────────────────────────────────────
OS="unknown"
ARCH="unknown"

detect_os_or_die() {
    case "$(uname -s 2>/dev/null || true)" in
        Darwin) OS="macos" ;;
        Linux)  OS="linux" ;;
        *)      OS="unsupported" ;;
    esac
    if [[ "$OS" == "unsupported" ]]; then
        ui_error "Unsupported operating system."
        echo "garagetytus v0.1 supports macOS and Linux only."
        echo "Windows targets v0.2 (see docs/install/windows.md)."
        exit 1
    fi
}

detect_arch() {
    case "$(uname -m 2>/dev/null || true)" in
        x86_64|amd64)   ARCH="x86_64" ;;
        arm64|aarch64)  ARCH="aarch64" ;;
        *)              ARCH="unknown" ;;
    esac
}

# ──────────────────────────────────────────────────────────────
# gum bootstrap (charmbracelet/gum). Optional — falls back to
# plain ANSI if download / TTY check fails.
# ──────────────────────────────────────────────────────────────
GUM_VERSION="${GARAGETYTUS_GUM_VERSION:-0.17.0}"
GUM=""
GUM_STATUS="skipped"
GUM_REASON=""

is_non_interactive() {
    [[ "${GARAGETYTUS_NO_PROMPT:-0}" == "1" ]] && return 0
    [[ ! -t 0 || ! -t 1 ]] && return 0
    return 1
}

gum_is_tty() {
    [[ -n "${NO_COLOR:-}" ]] && return 1
    [[ "${TERM:-dumb}" == "dumb" ]] && return 1
    [[ -t 2 || -t 1 ]] && return 0
    [[ -r /dev/tty && -w /dev/tty ]] && return 0
    return 1
}

gum_asset_os() {
    case "$(uname -s 2>/dev/null)" in
        Darwin) echo "Darwin" ;;
        Linux)  echo "Linux" ;;
        *)      echo "unsupported" ;;
    esac
}

gum_asset_arch() {
    case "$(uname -m 2>/dev/null)" in
        x86_64|amd64) echo "x86_64" ;;
        arm64|aarch64) echo "arm64" ;;
        *) echo "unknown" ;;
    esac
}

verify_sha256_file() {
    local checksums="$1"
    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum --ignore-missing -c "$checksums" >/dev/null 2>&1
        return $?
    fi
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 --ignore-missing -c "$checksums" >/dev/null 2>&1
        return $?
    fi
    return 1
}

bootstrap_gum_temp() {
    if is_non_interactive; then
        GUM_REASON="non-interactive shell"
        return 1
    fi
    if ! gum_is_tty; then
        GUM_REASON="terminal does not support gum UI"
        return 1
    fi
    if command -v gum >/dev/null 2>&1; then
        GUM="gum"
        GUM_STATUS="found"
        GUM_REASON="already installed"
        return 0
    fi
    if ! command -v tar >/dev/null 2>&1; then
        GUM_REASON="tar not found"
        return 1
    fi
    local g_os g_arch asset base d
    g_os="$(gum_asset_os)"
    g_arch="$(gum_asset_arch)"
    if [[ "$g_os" == "unsupported" || "$g_arch" == "unknown" ]]; then
        GUM_REASON="unsupported os/arch ($g_os/$g_arch)"
        return 1
    fi
    asset="gum_${GUM_VERSION}_${g_os}_${g_arch}.tar.gz"
    base="https://github.com/charmbracelet/gum/releases/download/v${GUM_VERSION}"
    d="$(mktempdir)"
    if ! download_file "${base}/${asset}" "$d/$asset"; then
        GUM_REASON="download failed"
        return 1
    fi
    if ! download_file "${base}/checksums.txt" "$d/checksums.txt"; then
        GUM_REASON="checksum unavailable"
        return 1
    fi
    if ! (cd "$d" && verify_sha256_file "checksums.txt"); then
        GUM_REASON="checksum mismatch"
        return 1
    fi
    if ! tar -xzf "$d/$asset" -C "$d" >/dev/null 2>&1; then
        GUM_REASON="extract failed"
        return 1
    fi
    local gp
    gp="$(find "$d" -type f -name gum 2>/dev/null | head -n1 || true)"
    if [[ -z "$gp" ]]; then
        GUM_REASON="binary missing after extract"
        return 1
    fi
    chmod +x "$gp" 2>/dev/null || true
    GUM="$gp"
    GUM_STATUS="installed"
    GUM_REASON="temp, verified, v${GUM_VERSION}"
    return 0
}

print_gum_status() {
    case "$GUM_STATUS" in
        found)     ui_success "gum available (${GUM_REASON})" ;;
        installed) ui_success "gum bootstrapped (${GUM_REASON})" ;;
        *)
            if [[ -n "$GUM_REASON" && "$GUM_REASON" != "non-interactive shell" ]]; then
                ui_info "gum skipped (${GUM_REASON})"
            fi
            ;;
    esac
}

# ──────────────────────────────────────────────────────────────
# UI helpers — gum if available, ANSI fallback otherwise.
# ──────────────────────────────────────────────────────────────
ui_info() {
    local msg="$*"
    if [[ -n "$GUM" ]]; then "$GUM" log --level info "$msg"
    else echo -e "${MUTED}·${NC} ${msg}"; fi
}

ui_warn() {
    local msg="$*"
    if [[ -n "$GUM" ]]; then "$GUM" log --level warn "$msg"
    else echo -e "${WARN}!${NC} ${msg}"; fi
}

ui_success() {
    local msg="$*"
    if [[ -n "$GUM" ]]; then
        local mark
        mark="$("$GUM" style --foreground "#22c55e" --bold "✓")"
        echo -e "${mark} ${msg}"
    else
        echo -e "${SUCCESS}✓${NC} ${msg}"
    fi
}

ui_error() {
    local msg="$*"
    if [[ -n "$GUM" ]]; then "$GUM" log --level error "$msg"
    else echo -e "${ERROR}✗${NC} ${msg}"; fi
}

ui_section() {
    local title="$1"
    if [[ -n "$GUM" ]]; then
        "$GUM" style --bold --foreground "#d97706" --padding "1 0" "$title"
    else
        echo
        echo -e "${ACCENT}${BOLD}${title}${NC}"
    fi
}

INSTALL_STAGE_TOTAL=3
INSTALL_STAGE_CURRENT=0
ui_stage() {
    local title="$1"
    INSTALL_STAGE_CURRENT=$((INSTALL_STAGE_CURRENT + 1))
    ui_section "[${INSTALL_STAGE_CURRENT}/${INSTALL_STAGE_TOTAL}] ${title}"
}

ui_kv() {
    local key="$1"
    local value="$2"
    if [[ -n "$GUM" ]]; then
        local kp vp
        kp="$("$GUM" style --foreground "#5a6480" --width 22 "$key")"
        vp="$("$GUM" style --bold "$value")"
        "$GUM" join --horizontal "$kp" "$vp"
    else
        printf "  ${MUTED}%-22s${NC} ${BOLD}%s${NC}\n" "$key" "$value"
    fi
}

ui_panel() {
    local content="$1"
    if [[ -n "$GUM" ]]; then
        "$GUM" style --border rounded --border-foreground "#5a6480" --padding "0 1" "$content"
    else
        echo "$content"
    fi
}

ui_celebrate() {
    local msg="$1"
    if [[ -n "$GUM" ]]; then
        "$GUM" style --bold --foreground "#22c55e" "$msg"
    else
        echo -e "${SUCCESS}${BOLD}${msg}${NC}"
    fi
}

run_with_spinner() {
    local title="$1"
    shift
    if [[ -n "$GUM" ]] && gum_is_tty; then
        local err_log
        err_log="$(mktempfile)"
        if "$GUM" spin --spinner dot --title "$title" -- "$@" 2>"$err_log"; then
            return 0
        fi
        local rc=$?
        if [[ -s "$err_log" ]] && grep -Eiq 'setrawmode' "$err_log"; then
            GUM=""
            "$@"
            return $?
        fi
        [[ -s "$err_log" ]] && cat "$err_log" >&2
        return "$rc"
    fi
    "$@"
}

# ──────────────────────────────────────────────────────────────
# Tagline pool — random rotating subtitle on the banner.
# ──────────────────────────────────────────────────────────────
TAGLINES=(
    "S3 on localhost, no cloud bill, no ceremony."
    "Your dev laptop just got an object store."
    "boto3 against 127.0.0.1:3900 — that easy."
    "Garage on the inside, MIT on the outside."
    "Local S3, real SigV4, zero subscription."
    "Buckets are the lingua franca of your laptop."
    "Powered by Garage. Wrapped in good taste."
    "AGPL stays in the basement; your code stays MIT."
    "rclone, aws-cli, pandas — they all just work."
    "Per-app TTL'd grants. Because least privilege."
    "The 'works on my machine' object store."
    "Compile once, copy bytes forever."
    "Three watchdogs and a Prometheus endpoint walk into a bar."
    "Disk-pressure hysteresis, because read-only is a feature."
    "kill -9 me and watch the auto-repair flow."
    "S3 envy without the AWS bill."
    "Your buckets, your laptop, your rules."
)

DEFAULT_TAGLINE="S3 on localhost, no cloud bill, no ceremony."

pick_tagline() {
    local count=${#TAGLINES[@]}
    if [[ "$count" -eq 0 ]]; then
        echo "$DEFAULT_TAGLINE"
        return
    fi
    local idx=$((RANDOM % count))
    echo "${TAGLINES[$idx]}"
}

print_installer_banner() {
    local tagline="$1"
    if [[ -n "$GUM" ]]; then
        local title sub hint card
        title="$("$GUM" style --foreground "#d97706" --bold "📦 garagetytus installer")"
        sub="$("$GUM" style --foreground "#8892b0" "$tagline")"
        hint="$("$GUM" style --foreground "#5a6480" "v0.1.0-rc2 — source-build")"
        card="$(printf '%s\n%s\n%s' "$title" "$sub" "$hint")"
        "$GUM" style --border rounded --border-foreground "#d97706" --padding "1 2" "$card"
        echo
        return
    fi
    echo
    echo -e "${ACCENT}${BOLD}  📦 garagetytus installer${NC}"
    echo -e "${INFO}  ${tagline}${NC}"
    echo -e "${MUTED}  v0.1.0-rc2 — source-build${NC}"
    echo
}

# ──────────────────────────────────────────────────────────────
# Prereq checks — Rust toolchain.
# ──────────────────────────────────────────────────────────────
RUST_VERSION=""
check_rust() {
    if ! command -v cargo >/dev/null 2>&1; then return 1; fi
    if ! command -v rustc >/dev/null 2>&1; then return 1; fi
    RUST_VERSION="$(rustc --version 2>/dev/null | awk '{print $2}')"
    [[ -n "$RUST_VERSION" ]]
}

ensure_rust_or_offer_rustup() {
    ui_warn "Rust toolchain not found on PATH."
    cat <<EOF
  garagetytus needs cargo + rustc 1.75+ to compile from source.
  Install rustup (the official Rust installer) with:

    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y

  After it finishes, source ~/.cargo/env (or open a new shell)
  and re-run this installer. We don't auto-run rustup because
  it modifies your shell init files; consent should be explicit.

EOF
    exit 1
}

# ──────────────────────────────────────────────────────────────
# Prereq checks — Garage daemon binary.
# ──────────────────────────────────────────────────────────────
GARAGE_VERSION=""
check_garage() {
    if ! command -v garage >/dev/null 2>&1; then return 1; fi
    GARAGE_VERSION="$(garage --version 2>/dev/null | head -n1 | awk '{print $NF}')"
    [[ -n "$GARAGE_VERSION" ]]
}

resolve_brew_bin() {
    if command -v brew >/dev/null 2>&1; then
        command -v brew
        return 0
    fi
    [[ -x "/opt/homebrew/bin/brew" ]] && { echo "/opt/homebrew/bin/brew"; return 0; }
    [[ -x "/usr/local/bin/brew" ]] && { echo "/usr/local/bin/brew"; return 0; }
    return 1
}

ensure_garage_mac_brew() {
    local brew_bin
    if ! brew_bin="$(resolve_brew_bin)"; then
        ui_error "Homebrew not found."
        echo "  garagetytus uses brew to install the Garage daemon on macOS"
        echo "  (Garage upstream ships no native Mac binary in v0.1)."
        echo "  Install Homebrew first: https://brew.sh — it requires"
        echo "  interactive sudo, so we don't auto-install it."
        exit 1
    fi
    eval "$("$brew_bin" shellenv)" 2>/dev/null || true
    ui_info "Installing Garage via Homebrew (compile-from-source, ~3-5 min on first run)…"
    run_with_spinner "brew install garage" "$brew_bin" install garage
    ui_success "Garage installed via Homebrew."
}

# Pinned upstream Garage musl binary SHA. Source: versions.toml.
# Update both files together when bumping Garage.
GARAGE_LINUX_VERSION="v2.3.0"
GARAGE_LINUX_SHA_X86_64="f98d317942bb341151a2775162016bb50cf86b865d0108de03eb5db16e2120cd"
GARAGE_LINUX_SHA_AARCH64="8ced2ad3040262571de08aa600959aa51f97576d55da7946fcde6f66140705e2"

ensure_garage_linux_musl() {
    local target sha url d
    case "$ARCH" in
        x86_64)  target="x86_64-unknown-linux-musl"; sha="$GARAGE_LINUX_SHA_X86_64" ;;
        aarch64) target="aarch64-unknown-linux-musl"; sha="$GARAGE_LINUX_SHA_AARCH64" ;;
        *)
            ui_error "Unsupported Linux arch: $ARCH"
            exit 1
            ;;
    esac
    url="https://garagehq.deuxfleurs.fr/_releases/${GARAGE_LINUX_VERSION}/${target}/garage"
    d="$(mktempdir)"
    ui_info "Downloading Garage ${GARAGE_LINUX_VERSION} (${target})…"
    if ! download_file "$url" "$d/garage"; then
        ui_error "Garage download failed from $url"
        exit 1
    fi
    ui_info "Verifying SHA-256…"
    local got
    if command -v sha256sum >/dev/null 2>&1; then
        got="$(sha256sum "$d/garage" | awk '{print $1}')"
    else
        got="$(shasum -a 256 "$d/garage" | awk '{print $1}')"
    fi
    if [[ "$got" != "$sha" ]]; then
        ui_error "Garage SHA mismatch — refusing to install."
        echo "  expected: $sha"
        echo "  got:      $got"
        echo "  This is a supply-chain integrity gate. File an issue:"
        echo "  https://github.com/traylinx/garagetytus/issues"
        exit 1
    fi
    chmod 0755 "$d/garage"
    mkdir -p "$HOME/.local/bin"
    install -m 0755 "$d/garage" "$HOME/.local/bin/garage"
    if ! command -v garage >/dev/null 2>&1; then
        ui_warn "$HOME/.local/bin is not on PATH."
        echo "  Add it before using garagetytus:"
        echo "      export PATH=\"\$HOME/.local/bin:\$PATH\""
    fi
    ui_success "Garage ${GARAGE_LINUX_VERSION} installed at \$HOME/.local/bin/garage"
}

# ──────────────────────────────────────────────────────────────
# garagetytus install — cargo install --git, with --force so
# re-runs upgrade cleanly to whatever HEAD points at.
# ──────────────────────────────────────────────────────────────
GARAGETYTUS_REPO_URL="https://github.com/traylinx/garagetytus.git"
GARAGETYTUS_REF="${GARAGETYTUS_REF:-main}"

install_garagetytus_via_cargo() {
    ui_info "Compiling garagetytus from source (this is the slow step — go grab coffee)…"
    local args=(install --git "$GARAGETYTUS_REPO_URL" --branch "$GARAGETYTUS_REF"
                --bin garagetytus --force --quiet)
    if [[ "${GARAGETYTUS_DRY_RUN:-0}" == "1" ]]; then
        ui_info "DRY RUN — would run: cargo ${args[*]}"
        return 0
    fi
    if ! cargo "${args[@]}"; then
        ui_error "cargo install failed."
        echo "  Re-run with verbose output to diagnose:"
        echo "      cargo install --git $GARAGETYTUS_REPO_URL --branch $GARAGETYTUS_REF --bin garagetytus --force"
        exit 1
    fi
    ui_success "garagetytus binary installed at \$HOME/.cargo/bin/garagetytus"
}

# ──────────────────────────────────────────────────────────────
# First-run wizard — chains `garagetytus install + start +
# bootstrap` after explicit consent. Skipped in non-interactive
# mode and when GARAGETYTUS_NO_ONBOARD=1.
# ──────────────────────────────────────────────────────────────
maybe_first_run() {
    if [[ "${GARAGETYTUS_NO_ONBOARD:-0}" == "1" ]] || is_non_interactive; then
        echo
        ui_info "Skipping first-run wizard (non-interactive)."
        print_next_steps_manual
        return
    fi
    echo
    local answer="n"
    if [[ -n "$GUM" ]]; then
        if "$GUM" confirm --default=true --affirmative="Yes, run it now" --negative="No, I'll run it later" \
            "Run \`garagetytus install + start + bootstrap\` now?"; then
            answer="y"
        fi
    else
        printf "Run \`garagetytus install + start + bootstrap\` now? [Y/n] "
        read -r answer || answer="y"
        case "$answer" in [Nn]*) answer="n" ;; *) answer="y" ;; esac
    fi
    if [[ "$answer" != "y" ]]; then
        print_next_steps_manual
        return
    fi
    local gtx="$HOME/.cargo/bin/garagetytus"
    [[ -x "$gtx" ]] || gtx="garagetytus"
    ui_section "First-run: garagetytus install"
    "$gtx" install || { ui_error "garagetytus install failed"; exit 1; }
    ui_section "First-run: garagetytus start"
    "$gtx" start || { ui_error "garagetytus start failed"; exit 1; }
    sleep 2
    ui_section "First-run: garagetytus bootstrap"
    "$gtx" bootstrap || { ui_error "garagetytus bootstrap failed"; exit 1; }
    echo
    ui_celebrate "garagetytus is up. http://127.0.0.1:3900 (S3) / :3904 (metrics)"
    echo
    ui_info "Try it:"
    echo "    garagetytus bucket create my-data --ttl 7d --quota 1G"
    echo "    garagetytus bucket grant my-data --to my-app --perms read,write --ttl 1h --json"
}

print_next_steps_manual() {
    echo
    ui_panel "$(cat <<EOF
Next steps:
  garagetytus install
  garagetytus start
  garagetytus bootstrap
  garagetytus bucket create my-data --ttl 7d --quota 1G

Manual:    https://github.com/traylinx/garagetytus/blob/main/docs/MANUAL.md
Skills:    https://github.com/traylinx/garagetytus/tree/main/skills
EOF
)"
}

# ──────────────────────────────────────────────────────────────
# Main.
# ──────────────────────────────────────────────────────────────
main() {
    detect_downloader
    bootstrap_gum_temp || true

    local tagline
    tagline="$(pick_tagline)"
    print_installer_banner "$tagline"
    print_gum_status

    detect_os_or_die
    detect_arch
    ui_success "Detected: $OS / $ARCH"

    ui_section "Install plan"
    ui_kv "OS"                "$OS"
    ui_kv "Arch"              "$ARCH"
    ui_kv "Method"            "source-build"
    ui_kv "Version"           "v0.1.0-rc2 (HEAD of $GARAGETYTUS_REF)"
    ui_kv "Estimated time"    "~3-5 min on first run"
    ui_kv "Downloader"        "$DOWNLOADER"
    if [[ "${GARAGETYTUS_DRY_RUN:-0}" == "1" ]]; then
        ui_kv "Dry run" "yes"
    fi

    if [[ "${GARAGETYTUS_DRY_RUN:-0}" == "1" ]]; then
        echo
        ui_info "Dry run — exiting without changes."
        return 0
    fi

    ui_stage "Preparing environment"
    if check_rust; then
        ui_success "Rust toolchain present (rustc $RUST_VERSION)"
    else
        ensure_rust_or_offer_rustup
    fi

    ui_stage "Installing Garage daemon (AGPL upstream)"
    if check_garage; then
        ui_success "garage already on PATH ($GARAGE_VERSION)"
    else
        case "$OS" in
            macos) ensure_garage_mac_brew ;;
            linux) ensure_garage_linux_musl ;;
        esac
    fi

    ui_stage "Installing garagetytus (cargo install --git)"
    install_garagetytus_via_cargo

    echo
    ui_celebrate "garagetytus installed."
    maybe_first_run
}

main "$@"
