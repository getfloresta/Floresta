# SPDX-License-Identifier: MIT OR Apache-2.0

"""
Functional tests for florestad's JSON-RPC authentication layer.

Covers the four configurations the daemon supports:
  - default cookie auth: anonymous request rejected, cookie request accepted
  - configured username/password: wrong password rejected
  - multi-user `-rpcauth`: pre-hashed entry accepted with the matching plaintext
"""

import os

from requests import post
from requests.models import HTTPBasicAuth

from test_framework.node import NodeType

# Pre-computed rpcauth triple, generated with Bitcoin Core's rpcauth.py algorithm
# (HMAC-SHA256 keyed by the salt's ASCII bytes over the plaintext password).
# Pinned in-tree so the test stays deterministic without invoking rpcauth.py at
# test time.
RPCAUTH_USER = "alice"
RPCAUTH_PASSWORD = "AlicePassword42"
RPCAUTH_SALT = "cf2c6b493e4690de904306d1a82ef1cc"
RPCAUTH_HASH = "1fa75db549cdb6b598c791f5db104d10919b6b0e6f2ebca11f66a348fe204727"
RPCAUTH_ENTRY = f"{RPCAUTH_USER}:{RPCAUTH_SALT}${RPCAUTH_HASH}"


def _rpc_url(node):
    cfg = node.rpc.config
    return f"http://{cfg.host}:{cfg.port}"


def _post_rpc(node, *, auth=None, headers=None):
    """Send a getblockcount POST and return the raw `requests.Response`."""
    body = b'{"jsonrpc":"2.0","id":"auth-test","method":"getblockcount","params":[]}'
    request_headers = {"content-type": "application/json"}
    if headers:
        request_headers.update(headers)
    return post(
        _rpc_url(node),
        data=body,
        headers=request_headers,
        auth=auth,
        timeout=15,
    )


def _start_florestad(node_manager, extra_args, load_cookie):
    """
    Register and launch a florestad node bypassing `Node.start()`. Default RPC
    credentials are cleared so `get_cmd_rpc` omits `--rpc-user`/`--rpc-password`,
    letting tests drive cookie auth or supply custom creds via `extra_args`.
    """
    node = node_manager.add_node_extra_args(
        variant=NodeType.FLORESTAD, extra_args=extra_args
    )
    node.rpc.config.user = None
    node.rpc.config.password = None
    node.daemon.start()
    node.rpc.wait_on_socket(opened=True)
    if load_cookie:
        cookie_path = os.path.join(node.daemon.data_dir, "regtest", ".cookie")
        with open(cookie_path, "r", encoding="utf-8") as fh:
            user, password = fh.read().split(":", 1)
        node.rpc.config.user = user
        node.rpc.config.password = password
    node.static_values = True
    return node


def _set_rpc_credentials(node, user, password):
    """Populate the RPC client's basic-auth credentials so node teardown's
    graceful `stop` call authenticates instead of relying on SIGTERM."""
    node.rpc.config.user = user
    node.rpc.config.password = password


class TestRpcAuth:
    """End-to-end checks of the JSON-RPC basic-auth gate."""

    def test_anonymous_request_rejected(self, node_manager):
        """A request without an Authorization header gets a 401 + WWW-Authenticate."""
        node = _start_florestad(node_manager, extra_args=[], load_cookie=True)

        resp = _post_rpc(node)

        assert resp.status_code == 401
        assert resp.headers.get("WWW-Authenticate") == 'Basic realm="jsonrpc"'
        assert resp.content == b""

    def test_cookie_request_accepted(self, node_manager):
        """The cookie file's `__cookie__:<token>` line authenticates a request."""
        node = _start_florestad(node_manager, extra_args=[], load_cookie=False)

        cookie_path = os.path.join(node.daemon.data_dir, "regtest", ".cookie")
        with open(cookie_path, "r", encoding="utf-8") as fh:
            cookie_line = fh.read()
        user, password = cookie_line.split(":", 1)
        assert user == "__cookie__"
        assert len(password) == 64

        resp = _post_rpc(node, auth=HTTPBasicAuth(user, password))

        assert resp.status_code == 200
        body = resp.json()
        assert body.get("error") is None
        assert isinstance(body.get("result"), int)

        _set_rpc_credentials(node, user, password)

    def test_wrong_password_rejected(self, node_manager):
        """Configured user/password rejects requests sent with a bogus password."""
        node = _start_florestad(
            node_manager,
            extra_args=["--rpc-user=alice", "--rpc-password=correct-horse-battery"],
            load_cookie=False,
        )

        resp = _post_rpc(node, auth=HTTPBasicAuth("alice", "definitely-wrong"))

        assert resp.status_code == 401
        assert resp.headers.get("WWW-Authenticate") == 'Basic realm="jsonrpc"'
        assert resp.content == b""

        _set_rpc_credentials(node, "alice", "correct-horse-battery")

    def test_rpcauth_multiuser_accepted(self, node_manager):
        """A pre-hashed `-rpcauth` entry authenticates with its plaintext password."""
        node = _start_florestad(
            node_manager,
            extra_args=[f"--rpc-auth={RPCAUTH_ENTRY}"],
            load_cookie=False,
        )

        resp = _post_rpc(node, auth=HTTPBasicAuth(RPCAUTH_USER, RPCAUTH_PASSWORD))

        assert resp.status_code == 200
        body = resp.json()
        assert body.get("error") is None
        assert isinstance(body.get("result"), int)

        _set_rpc_credentials(node, RPCAUTH_USER, RPCAUTH_PASSWORD)
