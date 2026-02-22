"""
Tests for JSON-RPC request parsing in florestad.

Validates that the RPC server correctly handles:
- Positional (array) parameters
- Named (object) parameters
- Null / omitted parameters
- Default values for optional parameters
- Proper JSON-RPC error codes per the spec (-32700, -32600, -32601, -32602, -32603)
- HTTP status codes (400, 404, 500, 503)
- Methods that require no params vs methods that require params
"""

import json

from requests import post
from test_framework import FlorestaTestFramework
from test_framework.node import NodeType


class RpcServerRequestParsingTest(FlorestaTestFramework):
    """
    Test JSON-RPC request parsing, parameter extraction (positional and named),
    error codes, and edge cases on the florestad RPC server.
    """

    def set_test_params(self):
        self.node = self.add_node_default_args(NodeType.FLORESTAD)

    def raw_request(self, payload: dict) -> dict:
        """
        Send a raw JSON-RPC request (as a dict) to the node and return the
        full parsed response body.  Does NOT raise on non-200 status codes
        so callers can inspect both the HTTP status and the JSON body.
        """
        url = f"{self.node.rpc.address}/"
        resp = post(
            url,
            headers={"content-type": "application/json"},
            data=json.dumps(payload),
            timeout=10,
        )
        return {"status_code": resp.status_code, "body": resp.json()}

    def raw_request_text(self, text: str) -> dict:
        """
        Send raw text (possibly invalid JSON) to the node.
        Returns the full response dict with status_code and body (parsed or raw).
        """
        url = f"{self.node.rpc.address}/"
        resp = post(
            url,
            headers={"content-type": "application/json"},
            data=text,
            timeout=10,
        )
        try:
            body = resp.json()
        except Exception:
            body = resp.text
        return {"status_code": resp.status_code, "body": body}

    # JSON-RPC spec error code constants (mirroring the server)
    PARSE_ERROR = -32700
    INVALID_REQUEST = -32600
    METHOD_NOT_FOUND = -32601
    INVALID_PARAMS = -32602
    INTERNAL_ERROR = -32603

    # ── Tests ─────────────────────────────────────────────────────────────

    def run_test(self):
        self.run_node(self.node)

        self.test_no_param_methods_without_params()

        self.test_no_param_methods_with_null_params()

        self.test_no_param_methods_with_empty_array()

        self.test_positional_params()

        self.test_named_params()

        self.test_optional_defaults()

        self.test_method_not_found()

        self.test_missing_required_params()

        # ── 9. Wrong parameter types ─────────────────────────────────
        self.test_wrong_param_types()

        # ── 10. Invalid JSON-RPC version ─────────────────────────────
        self.test_invalid_jsonrpc_version()

        # ── 11. Methods requiring params fail when params missing ────
        self.test_param_methods_fail_without_params()

        # ── 12. Response structure ───────────────────────────────────
        self.test_response_structure()

        self.stop()

    # ──────────────────────────────────────────────────────────────────
    # 1. No-param methods with params omitted
    # ──────────────────────────────────────────────────────────────────
    def test_no_param_methods_without_params(self):
        """Methods that need no params should succeed when 'params' is omitted."""
        self.log("Test: no-param methods without params field")

        no_param_methods = [
            "getbestblockhash",
            "getblockchaininfo",
            "getblockcount",
            "getroots",
            "getrpcinfo",
            "uptime",
            "getpeerinfo",
            "listdescriptors",
        ]

        for method in no_param_methods:
            resp = self.raw_request(
                {
                    "jsonrpc": "2.0",
                    "id": "test",
                    "method": method,
                }
            )

            self.assertEqual(
                resp["status_code"],
                200,
            )
            self.assertIsNone(resp["body"].get("error"))

    # ──────────────────────────────────────────────────────────────────
    # 2. No-param methods with params: null
    # ──────────────────────────────────────────────────────────────────
    def test_no_param_methods_with_null_params(self):
        """Methods that need no params should succeed when 'params' is null."""
        self.log("Test: no-param methods with params: null")

        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblockcount",
                "params": None,
            }
        )

        self.assertEqual(resp["status_code"], 200)
        self.assertIsNone(resp["body"].get("error"))

    # ──────────────────────────────────────────────────────────────────
    # 3. No-param methods with empty array
    # ──────────────────────────────────────────────────────────────────
    def test_no_param_methods_with_empty_array(self):
        """Methods that need no params should succeed with 'params': []."""
        self.log("Test: no-param methods with empty array params")

        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblockcount",
                "params": [],
            }
        )

        self.assertEqual(resp["status_code"], 200)
        self.assertIsNone(resp["body"].get("error"))

    # ──────────────────────────────────────────────────────────────────
    # 4. Positional (array) parameters
    # ──────────────────────────────────────────────────────────────────
    def test_positional_params(self):
        """Methods should accept positional (array) parameters."""
        self.log("Test: positional params")

        # getblockhash with positional param: height 0
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblockhash",
                "params": [0],
            }
        )

        self.assertEqual(resp["status_code"], 200)
        self.assertIsNone(resp["body"].get("error"))

        genesis_hash = resp["body"]["result"]

        # getblockheader with positional param: genesis hash
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblockheader",
                "params": [genesis_hash],
            }
        )

        self.assertEqual(resp["status_code"], 200)
        self.assertIsNone(resp["body"].get("error"))

        # getblock with positional params: hash, verbosity
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblock",
                "params": [genesis_hash, 1],
            }
        )

        self.assertEqual(resp["status_code"], 200)
        self.assertIsNone(resp["body"].get("error"))

    # ──────────────────────────────────────────────────────────────────
    # 5. Named (object) parameters
    # ──────────────────────────────────────────────────────────────────
    def test_named_params(self):
        """Methods should accept named (object) parameters."""
        self.log("Test: named params")

        # getblockhash with named param
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblockhash",
                "params": {"block_height": 0},
            }
        )

        self.assertEqual(resp["status_code"], 200)
        self.assertIsNone(resp["body"].get("error"))

        genesis_hash = resp["body"]["result"]

        # getblockheader with named param
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblockheader",
                "params": {"block_hash": genesis_hash},
            }
        )

        self.assertEqual(resp["status_code"], 200)
        self.assertIsNone(resp["body"].get("error"))

        # getblock with named params (including optional verbosity)
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblock",
                "params": {"block_hash": genesis_hash, "verbosity": 0},
            }
        )

        self.assertEqual(resp["status_code"], 200)
        self.assertIsNone(resp["body"].get("error"))

    # ──────────────────────────────────────────────────────────────────
    # 6. Optional / default parameters
    # ──────────────────────────────────────────────────────────────────
    def test_optional_defaults(self):
        """Optional parameters should use defaults when omitted."""
        self.log("Test: optional defaults")

        # Get genesis hash first
        genesis_hash = self.node.rpc.get_bestblockhash()

        # getblock with only the required param (verbosity defaults to 1)
        resp_default = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblock",
                "params": [genesis_hash],
            }
        )

        self.assertEqual(resp_default["status_code"], 200)
        self.assertIsNone(resp_default["body"].get("error"))

        # The result should be the verbose (verbosity=1) block representation,
        # which is an object, not a hex string.
        result = resp_default["body"]["result"]
        self.assertIn("hash", result)
        self.assertIn("tx", result)

        # getblock with explicit verbosity=1 should match the default
        resp_explicit = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblock",
                "params": [genesis_hash, 1],
            }
        )

        self.assertEqual(resp_explicit["status_code"], 200)
        self.assertEqual(
            resp_default["body"]["result"], resp_explicit["body"]["result"]
        )

        # getmemoryinfo, omitted default.
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getmemoryinfo",
            }
        )

        self.assertEqual(resp["status_code"], 200)
        self.assertIsNone(resp["body"].get("error"))

        # Named params: providing only the required field, optional uses default
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblock",
                "params": {"block_hash": genesis_hash},
            }
        )

        self.assertEqual(resp["status_code"], 200)
        self.assertIsNone(resp["body"].get("error"))
        self.assertIn("hash", resp["body"]["result"])

    # ──────────────────────────────────────────────────────────────────
    # 7. Method not found
    # ──────────────────────────────────────────────────────────────────
    def test_method_not_found(self):
        """Unknown methods should return METHOD_NOT_FOUND (-32601) and 404."""
        self.log("Test: method not found")

        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "nonexistent_method",
                "params": [],
            }
        )

        self.assertEqual(resp["status_code"], 404)
        self.assertIsSome(resp["body"].get("error"))
        self.assertEqual(resp["body"]["error"]["code"], self.METHOD_NOT_FOUND)

    def test_missing_required_params(self):
        """Methods with required params should return INVALID_PARAMS (-32602)."""
        self.log("Test: missing required params")

        # getblockhash requires a height parameter
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblockhash",
                "params": [],
            }
        )

        self.assertNotEqual(resp["status_code"], 200)
        self.assertIsSome(resp["body"].get("error"))
        self.assertEqual(resp["body"]["error"]["code"], self.INVALID_PARAMS)

        # getblockheader requires a block_hash parameter, not a int.
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblockheader",
                "params": [
                    1,
                ],
            }
        )

        self.assertNotEqual(resp["status_code"], 200)
        self.assertIsSome(resp["body"].get("error"))
        self.assertEqual(resp["body"]["error"]["code"], self.INVALID_PARAMS)

        # Named params: empty object also means missing required fields
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblockhash",
                "params": {},
            }
        )

        self.assertNotEqual(resp["status_code"], 200)
        self.assertIsSome(resp["body"].get("error"))
        self.assertEqual(resp["body"]["error"]["code"], self.INVALID_PARAMS)

    def test_wrong_param_types(self):
        """Passing the wrong type should return INVALID_PARAMS (-32602)."""
        self.log("Test: wrong param types")

        # getblockhash expects a number, not a string
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblockhash",
                "params": ["not_a_number"],
            }
        )

        self.assertNotEqual(resp["status_code"], 200)
        self.assertIsSome(resp["body"].get("error"))
        self.assertEqual(resp["body"]["error"]["code"], self.INVALID_PARAMS)

        # getblock expects a valid block hash string, not a number
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblock",
                "params": [12345],
            }
        )

        self.assertNotEqual(resp["status_code"], 200)
        self.assertIsSome(resp["body"].get("error"))

        # getblock verbosity expects a number, not a string
        genesis_hash = self.node.rpc.get_bestblockhash()
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "test",
                "method": "getblock",
                "params": [genesis_hash, "invalid_verbosity"],
            }
        )

        self.assertNotEqual(resp["status_code"], 200)
        self.assertIsSome(resp["body"].get("error"))
        self.assertEqual(resp["body"]["error"]["code"], self.INVALID_PARAMS)

    def test_invalid_jsonrpc_version(self):
        """An unsupported jsonrpc version should be rejected."""
        self.log("Test: invalid jsonrpc version")

        resp = self.raw_request(
            {
                "jsonrpc": "3.0",
                "id": "test",
                "method": "getblockcount",
                "params": [],
            }
        )

        self.assertNotEqual(resp["status_code"], 200)
        self.assertIsSome(resp["body"].get("error"))

        # Valid versions ("1.0" and "2.0") should work
        for version in ["1.0", "2.0"]:
            resp = self.raw_request(
                {
                    "jsonrpc": version,
                    "id": "test",
                    "method": "getblockcount",
                    "params": [],
                }
            )

            self.assertEqual(resp["status_code"], 200)
            self.assertIsNone(resp["body"].get("error"))

        # Omitted jsonrpc field should work (pre-2.0 compat)
        resp = self.raw_request(
            {
                "id": "test",
                "method": "getblockcount",
            }
        )

        self.assertEqual(resp["status_code"], 200)
        self.assertIsNone(resp["body"].get("error"))

    def test_param_methods_fail_without_params(self):
        """Methods that require params should fail when params is omitted."""
        self.log("Test: param methods fail without params")

        methods_needing_params = [
            "getblock",
            "getblockhash",
            "getblockheader",
            "getblockfrompeer",
            "getrawtransaction",
            "gettxout",
            "gettxoutproof",
            "findtxout",
            "addnode",
            "disconnectnode",
            "loaddescriptor",
            "sendrawtransaction",
        ]

        for method in methods_needing_params:
            resp = self.raw_request(
                {
                    "jsonrpc": "2.0",
                    "id": "test",
                    "method": method,
                }
            )

            self.assertNotEqual(
                resp["status_code"],
                200,
            )
            self.assertIsSome(resp["body"].get("error"))

    def test_response_structure(self):
        """
        Validate that responses follow the JSON-RPC 2.0 structure:
        - Success: {"result": ..., "error": null, "id": ...}
        - Error:   {"result": null, "error": {"code": ..., "message": ..., ...}, "id": ...}
        """
        self.log("Test: response structure")

        # Success case
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "struct_test",
                "method": "getblockcount",
            }
        )

        body = resp["body"]
        self.assertIn("result", body)
        self.assertIn("id", body)
        self.assertEqual(body["id"], "struct_test")
        # result should be present (not None), error should be absent or None
        self.assertIsSome(body.get("result"))

        # Error case
        resp = self.raw_request(
            {
                "jsonrpc": "2.0",
                "id": "struct_err",
                "method": "nonexistent",
                "params": [],
            }
        )

        body = resp["body"]
        self.assertIn("error", body)
        self.assertIn("id", body)
        self.assertEqual(body["id"], "struct_err")

        err = body["error"]
        self.assertIn("code", err)
        self.assertIn("message", err)
        # code must be an integer
        self.assertTrue(isinstance(err["code"], int))

        # The id should be echoed back even on errors
        self.assertEqual(body["id"], "struct_err")


if __name__ == "__main__":
    RpcServerRequestParsingTest().main()
