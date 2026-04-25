"""garagetytus — Python SDK for the local Garage S3 daemon.

Carved 2026-04-25 from
``MAKAKOO/plugins/lib-harvey-core/src/core/s3/__init__.py``
(376 LOC) per GARAGETYTUS-V0.1 Phase C. Two things changed:

1. Keychain backend is now the cross-platform ``keyring`` pip
   package (LD#5) instead of macOS-only
   ``subprocess(['/usr/bin/security', ...])``. The 11 unit tests
   already mocked the credential lookup, so they keep passing —
   only ``_service_creds`` and ``_grant_creds`` internals change.
   The new module ``_keychain`` wraps ``keyring``.

2. Brand renames:
   * Service namespace: ``makakoo`` → ``garagetytus``.
   * Service-creds account: ``makakoo-s3-service`` →
     ``s3-service``.
   * Env vars: ``GARAGETYTUS_PEER_NAME`` is read first; the legacy
     ``MAKAKOO_PEER_NAME`` is read as a fallback so existing pods
     keep working until the next install bootstrap.
   * Same for ``GARAGETYTUS_POD_WG_IP_OVERRIDE`` / legacy
     ``MAKAKOO_POD_WG_IP_OVERRIDE``.

Path-style addressing is locked unconditionally (LD#4 / LD#14).
"""

from __future__ import annotations

import contextlib
import os
from dataclasses import dataclass
from typing import Iterable, Iterator, Literal, Optional

from . import _keychain

# ── caller context ───────────────────────────────────────────


CallerKind = Literal["mac-local", "pod"]


@dataclass(frozen=True)
class CallerContext:
    """Where the calling code is running.

    Mac-local callers reach Garage directly at ``127.0.0.1:3900``.
    Pod-originated callers go via the MCP shim at
    ``<calling-pod-mac-wg-ip>:8765``.

    Auto-detection reads (in order):
      1. ``GARAGETYTUS_PEER_NAME``
      2. ``MAKAKOO_PEER_NAME`` (legacy — kept until v0.2 install
         pass migrates pods).
    """

    kind: CallerKind
    peer_name: Optional[str] = None

    @classmethod
    def from_runtime(cls) -> "CallerContext":
        peer = os.environ.get("GARAGETYTUS_PEER_NAME", "").strip()
        if not peer:
            peer = os.environ.get("MAKAKOO_PEER_NAME", "").strip()
        if peer:
            return cls(kind="pod", peer_name=peer)
        return cls(kind="mac-local")

    @classmethod
    def mac_local(cls) -> "CallerContext":
        return cls(kind="mac-local")

    @classmethod
    def pod(cls, peer_name: str) -> "CallerContext":
        return cls(kind="pod", peer_name=peer_name)

    def endpoint_url(self) -> str:
        if self.kind == "mac-local":
            return "http://127.0.0.1:3900"
        if self.kind == "pod":
            wg_ip = _resolve_pod_wg_ip(self.peer_name or "")
            return f"http://{wg_ip}:8765"
        raise ValueError(f"unknown CallerKind: {self.kind!r}")


_WG_IP_CACHE: dict[str, tuple[float, str]] = {}
_WG_IP_CACHE_TTL_S = 30.0


def _resolve_pod_wg_ip(peer_name: str) -> str:
    """Resolve the Mac-side WG IP for a pod identified by `peer_name`.

    Resolution order (first hit wins):

    1. ``GARAGETYTUS_POD_WG_IP_OVERRIDE`` / ``MAKAKOO_POD_WG_IP_OVERRIDE``
       env var — escape hatch for tests + non-`pod-NN` naming.
    2. The pod_id encoded in the peer name. Convention is
       ``pod-NN`` where ``NN`` is the 2-digit pod number; the
       Mac-side WG endpoint is ``10.18.<NN>.2``.
    3. Fallback to the canonical first-pod IP ``10.18.0.2``.

    Cached for 30 s per peer.
    """
    if not peer_name:
        raise ValueError("CallerContext.pod() requires non-empty peer_name")

    override = os.environ.get("GARAGETYTUS_POD_WG_IP_OVERRIDE") or os.environ.get(
        "MAKAKOO_POD_WG_IP_OVERRIDE"
    )
    if override:
        return override

    import time as _time

    now = _time.time()
    cached = _WG_IP_CACHE.get(peer_name)
    if cached is not None and (now - cached[0]) < _WG_IP_CACHE_TTL_S:
        return cached[1]

    ip = _wg_ip_from_peer_name(peer_name)
    _WG_IP_CACHE[peer_name] = (now, ip)
    return ip


def _wg_ip_from_peer_name(peer_name: str) -> str:
    """Pure helper — parse `pod-NN` shape, return `10.18.NN.2`."""
    if peer_name.startswith("pod-"):
        suffix = peer_name[4:]
        if suffix.isdigit():
            n = int(suffix)
            if 0 <= n <= 255:
                return f"10.18.{n}.2"
    return "10.18.0.2"


def _reset_wg_ip_cache() -> None:
    """Test hook — wipe the WG-IP resolution cache."""
    _WG_IP_CACHE.clear()


# ── credential resolution ────────────────────────────────────


# Account names within the SERVICE="garagetytus" keychain namespace.
_SERVICE_ACCOUNT = "s3-service"
_GRANT_ACCOUNT_PREFIX = "bucket-grant:"


def _service_creds() -> dict:
    """Return the bootstrapped ``s3-service`` creds.

    Raises a clear error pointing at ``garagetytus bootstrap`` if
    the keypair isn't there yet.
    """
    try:
        blob = _keychain.get_json(_SERVICE_ACCOUNT)
    except RuntimeError as e:
        raise RuntimeError(
            f"garagetytus-sdk: s3-service keypair not in keychain. "
            "Run `garagetytus install && garagetytus start && "
            "garagetytus bootstrap` first.\n"
            f"  underlying: {e}"
        ) from e
    return {"access_key": blob["access_key"], "secret_key": blob["secret_key"]}


def _grant_creds(grant_id: str) -> dict:
    """Resolve creds for a ``garagetytus bucket grant``-issued sub-keypair."""
    account = f"{_GRANT_ACCOUNT_PREFIX}{grant_id}"
    try:
        blob = _keychain.get_json(account)
    except RuntimeError as e:
        raise RuntimeError(
            f"garagetytus-sdk: grant {grant_id!r} not in keychain. "
            "Did you revoke the grant already?\n"
            f"  underlying: {e}"
        ) from e
    return blob


# ── client ──────────────────────────────────────────────────


def client(
    *,
    caller_context: Optional[CallerContext] = None,
    grant_id: Optional[str] = None,
    access_key: Optional[str] = None,
    secret_key: Optional[str] = None,
):
    """Return a boto3 S3 client wired to the resolved garagetytus
    endpoint.

    Credential resolution order:
      1. Explicit ``access_key`` + ``secret_key`` arguments win.
      2. ``grant_id`` looks up the sub-keypair in the keychain.
      3. Otherwise, the bootstrapped ``s3-service`` keypair.

    Endpoint URL comes from ``caller_context``; auto-detected via
    ``CallerContext.from_runtime()`` if ``None``.

    Path-style addressing is locked unconditionally (LD#4).
    """
    try:
        import boto3  # type: ignore[import-not-found]
        from botocore.config import Config  # type: ignore[import-not-found]
    except ImportError as e:
        raise RuntimeError(
            f"garagetytus-sdk: boto3 not available in venv ({e}). "
            "Install with: pip install garagetytus-sdk"
        )

    ctx = caller_context or CallerContext.from_runtime()

    if access_key and secret_key:
        ak, sk = access_key, secret_key
    elif grant_id:
        creds = _grant_creds(grant_id)
        ak, sk = creds["access_key"], creds["secret_key"]
    else:
        creds = _service_creds()
        ak, sk = creds["access_key"], creds["secret_key"]

    return boto3.client(
        "s3",
        endpoint_url=ctx.endpoint_url(),
        region_name="garage",
        aws_access_key_id=ak,
        aws_secret_access_key=sk,
        config=Config(s3={"addressing_style": "path"}),
    )


def async_client(*_args, **_kwargs):
    """Async variant — defers to v0.2.

    The v0.7.1 codebase ships sync boto3 only; aioboto3 lands when
    a real async caller surfaces.
    """
    raise NotImplementedError(
        "garagetytus.async_client lands in v0.2 — use sync `client()` until then."
    )


# ── bucket facade ────────────────────────────────────────────


class Bucket:
    """Convenience wrapper for one bucket.

    Auto-creates the bucket on first use (mirrors the v0.7 D.1
    behavior so existing call-sites can swap inline boto3 for this
    facade without changing semantics).
    """

    def __init__(self, name: str, s3_client) -> None:
        self._name = name
        self._client = s3_client
        self._ensured = False

    def _ensure(self) -> None:
        if self._ensured:
            return
        try:
            self._client.head_bucket(Bucket=self._name)
        except Exception:
            try:
                self._client.create_bucket(Bucket=self._name)
            except Exception as exc:  # noqa: BLE001 — boto3 raises broadly
                msg = str(exc)
                if (
                    "BucketAlreadyOwnedByYou" not in msg
                    and "BucketAlreadyExists" not in msg
                ):
                    raise
        self._ensured = True

    @property
    def name(self) -> str:
        return self._name

    def put(self, key: str, body: bytes | str, **extra) -> dict:
        self._ensure()
        if isinstance(body, str):
            body = body.encode("utf-8")
        return self._client.put_object(
            Bucket=self._name, Key=key, Body=body, **extra
        )

    def get(self, key: str) -> bytes:
        out = self._client.get_object(Bucket=self._name, Key=key)
        return out["Body"].read()

    def list(self, prefix: str = "", max_keys: int = 1000) -> Iterable[str]:
        out = self._client.list_objects_v2(
            Bucket=self._name, Prefix=prefix, MaxKeys=max_keys
        )
        for obj in out.get("Contents") or ():
            yield obj["Key"]

    def delete(self, key: str) -> dict:
        return self._client.delete_object(Bucket=self._name, Key=key)

    def presign(
        self, key: str, ttl: int = 600, mode: Literal["get", "put"] = "get"
    ) -> str:
        method = "get_object" if mode == "get" else "put_object"
        return self._client.generate_presigned_url(
            method,
            Params={"Bucket": self._name, "Key": key},
            ExpiresIn=ttl,
        )


@contextlib.contextmanager
def bucket_ctx(
    name: str,
    *,
    caller_context: Optional[CallerContext] = None,
    grant_id: Optional[str] = None,
    access_key: Optional[str] = None,
    secret_key: Optional[str] = None,
) -> Iterator[Bucket]:
    """Context-manager wrapper. Always yields a fresh ``Bucket`` for
    isolation; the underlying boto3 client is also fresh per call.
    """
    s3 = client(
        caller_context=caller_context,
        grant_id=grant_id,
        access_key=access_key,
        secret_key=secret_key,
    )
    yield Bucket(name, s3)


__all__ = [
    "Bucket",
    "CallerContext",
    "async_client",
    "bucket_ctx",
    "client",
]
