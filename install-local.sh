#!/usr/bin/env bash
#
# Build this checkout, sign it with a stable local identity, install it over
# the running paneru, and restart the service.
#
# macOS keys Accessibility (TCC) grants to a binary's code signature. An
# ad-hoc signature (`codesign -s -`) is different on every build, so every
# rebuild looks like a brand-new app: the grant is lost and a duplicate entry
# piles up in System Settings. Signing every build with ONE self-signed
# identity makes them all the same app to TCC, so a single approval sticks
# forever.
#
# The identity is created on first run and stored in ~/.config/paneru/signing.
#
# Usage: ./install-local.sh [--no-restart]

set -euo pipefail

readonly IDENTITY="Paneru Local Builds"
readonly SIGNING_DIR="${HOME}/.config/paneru/signing"
readonly CERT="${SIGNING_DIR}/paneru-cert.pem"
readonly KEY="${SIGNING_DIR}/paneru-key.pem"
readonly KEYCHAIN="${HOME}/Library/Keychains/login.keychain-db"
readonly SERVICE="com.github.karinushka.paneru"
readonly REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

log() { printf '==> %s\n' "$*"; }
die() { printf 'error: %s\n' "$*" >&2; exit 1; }

# Creates the self-signed code-signing identity and trusts it for code
# signing in the user's login keychain. Idempotent: does nothing once the
# identity is present.
ensure_identity() {
  if security find-identity -v -p codesigning | grep -qF "${IDENTITY}"; then
    return
  fi

  log "creating local code-signing identity '${IDENTITY}'"
  mkdir -p "${SIGNING_DIR}"

  if [[ ! -f "${CERT}" || ! -f "${KEY}" ]]; then
    local config
    config="$(mktemp)"
    cat >"${config}" <<EOF
[req]
distinguished_name = dn
x509_extensions = v3
prompt = no
[dn]
CN = ${IDENTITY}
[v3]
basicConstraints = critical,CA:FALSE
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
EOF
    openssl req -x509 -newkey rsa:2048 -keyout "${KEY}" -out "${CERT}" \
      -days 7300 -nodes -config "${config}" >/dev/null 2>&1
    rm -f "${config}"
    chmod 600 "${KEY}"
  fi

  security import "${CERT}" -k "${KEYCHAIN}" -A >/dev/null
  security import "${KEY}" -k "${KEYCHAIN}" -T /usr/bin/codesign -A >/dev/null
  # Prompts once for the login password: macOS will not trust a certificate
  # for code signing without explicit consent.
  log "macOS will ask for your password to trust the new certificate"
  security add-trusted-cert -r trustRoot -p codeSign -k "${KEYCHAIN}" "${CERT}"

  security find-identity -v -p codesigning | grep -qF "${IDENTITY}" \
    || die "identity '${IDENTITY}' was not created"
}

# Resolves the binary the launchd service actually executes, following the
# Homebrew symlink so we overwrite the file TCC has a grant for.
service_binary() {
  local program
  program="$(/usr/libexec/PlistBuddy -c 'Print :Program' \
    "${HOME}/Library/LaunchAgents/${SERVICE}.plist" 2>/dev/null || true)"
  [[ -n "${program}" ]] || die "no LaunchAgent found for ${SERVICE}"
  # Follow symlinks; Homebrew's bin/paneru points into the Cellar.
  python3 -c 'import os,sys; print(os.path.realpath(sys.argv[1]))' "${program}"
}

main() {
  local restart=1
  [[ "${1:-}" == "--no-restart" ]] && restart=0

  ensure_identity

  log "building release binary"
  (cd "${REPO_ROOT}" && cargo build --release)
  local built="${REPO_ROOT}/target/release/paneru"
  [[ -x "${built}" ]] || die "build produced no binary at ${built}"

  local target
  target="$(service_binary)"
  log "installing to ${target}"
  # The Cellar copy is read-only; restore the mode afterwards.
  local mode
  mode="$(stat -f '%Lp' "${target}")"
  chmod u+w "${target}"
  cp "${built}" "${target}"
  chmod "${mode}" "${target}"

  log "signing as '${IDENTITY}'"
  codesign --force --sign "${IDENTITY}" "${target}"

  if (( restart )); then
    log "restarting ${SERVICE}"
    launchctl kickstart -k "gui/$(id -u)/${SERVICE}"
  fi

  log "done. Accessibility approval carries over to every future build."
}

main "$@"
