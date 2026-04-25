"""Unit tests for ``garagetytus.CallerContext`` resolution.

Carved from the original ``core.s3.tests.test_caller_context`` per
Phase C.4. Imports updated to ``from garagetytus import …``; tests
exercise both the new ``GARAGETYTUS_PEER_NAME`` env var and the
legacy ``MAKAKOO_PEER_NAME`` fallback.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock


class CallerContextTests(unittest.TestCase):
    def test_default_is_mac_local_when_env_unset(self) -> None:
        from garagetytus import CallerContext

        env = {
            k: v
            for k, v in os.environ.items()
            if k not in ("MAKAKOO_PEER_NAME", "GARAGETYTUS_PEER_NAME")
        }
        with mock.patch.dict(os.environ, env, clear=True):
            ctx = CallerContext.from_runtime()
            self.assertEqual(ctx.kind, "mac-local")
            self.assertIsNone(ctx.peer_name)

    def test_pod_when_env_set_via_new_name(self) -> None:
        from garagetytus import CallerContext

        with mock.patch.dict(os.environ, {"GARAGETYTUS_PEER_NAME": "pod-02"}):
            ctx = CallerContext.from_runtime()
            self.assertEqual(ctx.kind, "pod")
            self.assertEqual(ctx.peer_name, "pod-02")

    def test_pod_when_env_set_via_legacy_name(self) -> None:
        from garagetytus import CallerContext

        env = {
            k: v
            for k, v in os.environ.items()
            if k != "GARAGETYTUS_PEER_NAME"
        }
        env["MAKAKOO_PEER_NAME"] = "pod-02"
        with mock.patch.dict(os.environ, env, clear=True):
            ctx = CallerContext.from_runtime()
            self.assertEqual(ctx.kind, "pod")
            self.assertEqual(ctx.peer_name, "pod-02")

    def test_explicit_constructors(self) -> None:
        from garagetytus import CallerContext

        mac = CallerContext.mac_local()
        self.assertEqual(mac.kind, "mac-local")
        pod = CallerContext.pod("pod-04")
        self.assertEqual(pod.kind, "pod")
        self.assertEqual(pod.peer_name, "pod-04")

    def test_endpoint_url_mac_local(self) -> None:
        from garagetytus import CallerContext

        ctx = CallerContext.mac_local()
        self.assertEqual(ctx.endpoint_url(), "http://127.0.0.1:3900")

    def test_endpoint_url_pod_uses_shim_port(self) -> None:
        from garagetytus import CallerContext, _reset_wg_ip_cache

        _reset_wg_ip_cache()
        ctx = CallerContext.pod("pod-02")
        url = ctx.endpoint_url()
        self.assertEqual(url, "http://10.18.2.2:8765")

    def test_endpoint_url_pod_per_pod_resolution(self) -> None:
        from garagetytus import CallerContext, _reset_wg_ip_cache

        _reset_wg_ip_cache()
        for n in (0, 2, 4, 7, 99):
            url = CallerContext.pod(f"pod-{n}").endpoint_url()
            self.assertEqual(url, f"http://10.18.{n}.2:8765")

    def test_endpoint_url_pod_unknown_peer_falls_back(self) -> None:
        from garagetytus import CallerContext, _reset_wg_ip_cache

        _reset_wg_ip_cache()
        url = CallerContext.pod("custom-named-pod").endpoint_url()
        self.assertEqual(url, "http://10.18.0.2:8765")

    def test_endpoint_url_pod_override_via_new_env(self) -> None:
        from garagetytus import CallerContext, _reset_wg_ip_cache

        _reset_wg_ip_cache()
        with mock.patch.dict(
            os.environ, {"GARAGETYTUS_POD_WG_IP_OVERRIDE": "10.99.99.99"}
        ):
            url = CallerContext.pod("pod-02").endpoint_url()
            self.assertEqual(url, "http://10.99.99.99:8765")

    def test_endpoint_url_pod_override_via_legacy_env(self) -> None:
        from garagetytus import CallerContext, _reset_wg_ip_cache

        _reset_wg_ip_cache()
        env = {
            k: v
            for k, v in os.environ.items()
            if k != "GARAGETYTUS_POD_WG_IP_OVERRIDE"
        }
        env["MAKAKOO_POD_WG_IP_OVERRIDE"] = "10.55.55.55"
        with mock.patch.dict(os.environ, env, clear=True):
            url = CallerContext.pod("pod-02").endpoint_url()
            self.assertEqual(url, "http://10.55.55.55:8765")

    def test_endpoint_url_pod_requires_peer_name(self) -> None:
        from garagetytus import CallerContext

        with self.assertRaises(ValueError):
            CallerContext.pod("").endpoint_url()


class AsyncStubTests(unittest.TestCase):
    def test_async_client_raises_v0_2_pointer(self) -> None:
        from garagetytus import async_client

        with self.assertRaises(NotImplementedError) as cm:
            async_client()
        msg = str(cm.exception)
        self.assertIn("v0.2", msg)


if __name__ == "__main__":
    unittest.main()
