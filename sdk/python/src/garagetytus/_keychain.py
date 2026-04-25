"""Cross-platform keychain abstraction (Phase C.2).

Carved 2026-04-25 to replace the macOS-only
``subprocess(['/usr/bin/security', ...])`` calls in the original
``core.s3`` module. Per LD#5, the keychain backend is the
``keyring`` pip package — supports macOS Keychain, Linux Secret
Service, Windows Credential Manager, and a documented
``keyring.backends.fail`` fallback for headless Linux.

LD#6 — headless-Linux fallback is opt-in via a CLI flag at the
``garagetytus install`` surface. This module DOES NOT silently
fall back; missing keychain raises ``KeychainUnavailableError``.
"""

from __future__ import annotations

import json as _json
from typing import Optional

# Service namespace — must match the Rust SecretsStore (LD#5 lockstep).
SERVICE = "garagetytus"


class KeychainUnavailableError(RuntimeError):
    """Raised when the OS keychain backend cannot be reached.

    On headless Linux without Secret Service, the keyring package
    raises ``keyring.errors.NoKeyringError`` or returns a fail
    backend; we surface a clear error pointing at the
    ``--allow-file-creds`` install flag.
    """


def get_password(account: str) -> Optional[str]:
    """Return the password stored under (SERVICE, account), or None
    if no entry exists. Raises ``KeychainUnavailableError`` if the
    keychain backend is broken (not just missing entry).
    """
    try:
        import keyring
    except ImportError as e:
        raise KeychainUnavailableError(
            f"garagetytus-sdk: `keyring` package not available ({e}). "
            "Install with: pip install garagetytus-sdk"
        ) from e
    try:
        return keyring.get_password(SERVICE, account)
    except keyring.errors.KeyringError as e:
        raise KeychainUnavailableError(
            f"garagetytus-sdk: cannot reach OS keychain — {e}. "
            "Re-run `garagetytus install --allow-file-creds` if "
            "you're on a headless Linux box without Secret Service."
        ) from e


def set_password(account: str, password: str) -> None:
    import keyring
    keyring.set_password(SERVICE, account, password)


def delete_password(account: str) -> None:
    import keyring
    try:
        keyring.delete_password(SERVICE, account)
    except keyring.errors.PasswordDeleteError:
        # Idempotent — missing entry is success.
        pass


def get_json(account: str) -> dict:
    """Convenience: fetch + json.loads + raise if missing."""
    raw = get_password(account)
    if raw is None:
        raise RuntimeError(
            f"garagetytus-sdk: no keychain entry for "
            f"({SERVICE!r}, {account!r}). Did you run "
            "`garagetytus bootstrap`?"
        )
    return _json.loads(raw)
