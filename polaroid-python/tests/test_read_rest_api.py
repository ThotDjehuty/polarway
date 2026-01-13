from __future__ import annotations

from dataclasses import dataclass

import pytest

import polaroid
from polaroid.client import PolaroidClient
from polaroid import polaroid_pb2


@dataclass
class _CapturedCall:
    request: object | None = None
    timeout: float | None = None


class _FakeStub:
    def __init__(self, *, handle: str = "h1", error: str | None = None):
        self.captured = _CapturedCall()
        self._handle = handle
        self._error = error

    def ReadRestApi(self, request, timeout=None):  # noqa: N802 (grpc style)
        self.captured.request = request
        self.captured.timeout = timeout
        if self._error is None:
            return polaroid_pb2.DataFrameHandle(handle=self._handle)
        return polaroid_pb2.DataFrameHandle(handle="", error=self._error)


def test_client_read_rest_api_builds_request_and_returns_dataframe_handle():
    client = PolaroidClient("localhost:50051")
    fake = _FakeStub(handle="abc123")
    client.stub = fake

    df = client.read_rest_api(
        url="https://example.com/data",
        method="POST",
        headers={"User-Agent": "pytest", "X-Test": "1"},
        body="{\"hello\":\"world\"}",
    )

    assert df.handle == "abc123"

    req = fake.captured.request
    assert isinstance(req, polaroid_pb2.RestApiRequest)
    assert req.url == "https://example.com/data"
    assert req.method == "POST"
    assert dict(req.headers) == {"User-Agent": "pytest", "X-Test": "1"}
    assert req.body == "{\"hello\":\"world\"}"


def test_client_read_rest_api_raises_on_server_error():
    client = PolaroidClient("localhost:50051")
    client.stub = _FakeStub(error="HTTP 403")

    with pytest.raises(RuntimeError, match="Server error: HTTP 403"):
        client.read_rest_api(url="https://example.com/data")


def test_package_level_read_rest_api_uses_default_client(monkeypatch: pytest.MonkeyPatch):
    # Avoid creating a real channel by faking the default client.
    class FakeDefaultClient:
        def __init__(self):
            self.calls = []

        def read_rest_api(self, url: str, method: str = "GET", headers=None, body=None):
            self.calls.append((url, method, headers, body))
            return "df-handle"

    fake_client = FakeDefaultClient()

    import polaroid.client as pld_client

    monkeypatch.setattr(pld_client, "_default_client", fake_client, raising=True)

    out = polaroid.read_rest_api(
        "https://example.com/data",
        method="GET",
        headers={"User-Agent": "pytest"},
        body=None,
    )

    assert out == "df-handle"
    assert fake_client.calls == [("https://example.com/data", "GET", {"User-Agent": "pytest"}, None)]
