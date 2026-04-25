"""Smoke — proves the SDK's ``client()`` builder produces a boto3
client wired with the right endpoint, region, creds, and addressing
style. Carved from ``core.s3.tests.test_dogfood`` per Phase C.4;
the AC12 ``test_mcp_dogfood_delegates_to_sdk`` test stays in
MAKAKOO/lib-harvey-core (it's a Makakoo-side contract that the MCP
handler delegates to garagetytus.client, not duplicated here).

Tests stay env-pure — no real keychain, no real boto3 connection.
"""

from __future__ import annotations

import os
import unittest
from unittest import mock


class DogfoodTests(unittest.TestCase):
    def _patched_creds(self):
        return {"access_key": "TEST-AK", "secret_key": "TEST-SK"}

    def test_client_passes_path_style_and_resolved_endpoint(self) -> None:
        captured = {}

        def fake_boto3_client(service, **kwargs):
            captured["service"] = service
            captured.update(kwargs)
            return "<fake-client>"

        env = {
            k: v
            for k, v in os.environ.items()
            if k not in ("MAKAKOO_PEER_NAME", "GARAGETYTUS_PEER_NAME")
        }
        with mock.patch.dict(os.environ, env, clear=True):
            with mock.patch("garagetytus._service_creds", self._patched_creds):
                with mock.patch.dict(
                    "sys.modules",
                    {
                        "boto3": mock.MagicMock(client=fake_boto3_client),
                        "botocore.config": mock.MagicMock(
                            Config=lambda **kw: ("FAKE-CONFIG", kw)
                        ),
                    },
                ):
                    from garagetytus import client

                    out = client()

        self.assertEqual(out, "<fake-client>")
        self.assertEqual(captured["service"], "s3")
        self.assertEqual(captured["endpoint_url"], "http://127.0.0.1:3900")
        self.assertEqual(captured["region_name"], "garage")
        self.assertEqual(captured["aws_access_key_id"], "TEST-AK")
        self.assertEqual(captured["aws_secret_access_key"], "TEST-SK")
        cfg_kw = captured["config"][1]
        self.assertEqual(cfg_kw["s3"], {"addressing_style": "path"})

    def test_client_uses_pod_endpoint_when_peer_name_set(self) -> None:
        captured = {}

        def fake_boto3_client(service, **kwargs):
            captured.update(kwargs)
            return "<fake-client>"

        from garagetytus import _reset_wg_ip_cache

        _reset_wg_ip_cache()

        with mock.patch.dict(os.environ, {"GARAGETYTUS_PEER_NAME": "pod-02"}):
            with mock.patch("garagetytus._service_creds", self._patched_creds):
                with mock.patch.dict(
                    "sys.modules",
                    {
                        "boto3": mock.MagicMock(client=fake_boto3_client),
                        "botocore.config": mock.MagicMock(
                            Config=lambda **kw: ("FAKE-CONFIG", kw)
                        ),
                    },
                ):
                    from garagetytus import client

                    client()
        self.assertEqual(captured["endpoint_url"], "http://10.18.2.2:8765")

    def test_explicit_creds_win_over_keychain(self) -> None:
        captured = {}

        def fake_boto3_client(service, **kwargs):
            captured.update(kwargs)
            return "<fake-client>"

        def patched_creds():
            return {"access_key": "KEYCHAIN-AK", "secret_key": "KEYCHAIN-SK"}

        env = {
            k: v
            for k, v in os.environ.items()
            if k not in ("MAKAKOO_PEER_NAME", "GARAGETYTUS_PEER_NAME")
        }
        with mock.patch.dict(os.environ, env, clear=True):
            with mock.patch("garagetytus._service_creds", patched_creds):
                with mock.patch.dict(
                    "sys.modules",
                    {
                        "boto3": mock.MagicMock(client=fake_boto3_client),
                        "botocore.config": mock.MagicMock(
                            Config=lambda **kw: ("FAKE-CONFIG", kw)
                        ),
                    },
                ):
                    from garagetytus import client

                    client(access_key="EXPLICIT-AK", secret_key="EXPLICIT-SK")
        self.assertEqual(captured["aws_access_key_id"], "EXPLICIT-AK")
        self.assertEqual(captured["aws_secret_access_key"], "EXPLICIT-SK")


if __name__ == "__main__":
    unittest.main()
