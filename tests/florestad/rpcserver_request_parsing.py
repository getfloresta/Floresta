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

from test_framework import FlorestaTestFramework
from test_framework.node import NodeType

# JSON-RPC spec error code constants
PARSE_ERROR = -32700
INVALID_REQUEST = -32600
METHOD_NOT_FOUND = -32601
INVALID_PARAMS = -32602
INTERNAL_ERROR = -32603


class RpcServerRequestParsingTest(FlorestaTestFramework):
    """
    Test JSON-RPC request parsing, parameter extraction (positional and named),
    error codes, and edge cases on the florestad RPC server.
    """

    def assert_success(self, resp):
        """Assert that a JSON-RPC response indicates success (HTTP 200, no error)."""
        self.assertEqual(resp["status_code"], 200)
        self.assertIsNone(resp["body"].get("error"))

    def assert_error(
        self, resp, expected_status_code=None, expected_rpcerror_code=None
    ):
        """
        Assert that a JSON-RPC response indicates an error (non-200, error present)."""
        self.assertIsSome(resp["body"].get("error"))

        if expected_status_code is None:
            self.assertNotEqual(resp["status_code"], 200)
        else:
            self.assertEqual(resp["status_code"], expected_status_code)

        if expected_rpcerror_code is not None:
            self.assertEqual(resp["body"]["error"]["code"], expected_rpcerror_code)

    def set_test_params(self):
        """Configure test parameters with a single florestad node."""
        self.node = self.add_node_default_args(NodeType.FLORESTAD)

    def run_test(self):
        """Run all JSON-RPC request parsing tests."""
        self.run_node(self.node)

        self.test_noparammethods_omittedparams_succeeds()
        self.test_noparammethods_nullparams_succeeds()
        self.test_noparammethods_emptyarray_succeeds()
        self.test_positionalparams_validargs_succeeds()
        self.test_namedparams_validargs_succeeds()
        self.test_optionalparams_omitted_usesdefaults()
        self.test_unknownmethod_anyparams_returnsmethodnotfound()
        self.test_requiredparams_missing_returnsinvalidparams()
        self.test_paramtypes_wrongtype_returnsinvalidparams()
        self.test_jsonrpcversion_invalid_returnsrejection()
        self.test_parammethods_omittedparams_returnserror()
        self.test_responsestructure_success_matchesjsonrpcspec()
        self.test_responsestructure_error_matchesjsonrpcspec()

    def test_noparammethods_omittedparams_succeeds(self):
        """Verify no-param methods succeed when the params field is omitted."""
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
            resp = self.node.rpc.noraise_request(method)
            self.assert_success(resp)

    def test_noparammethods_nullparams_succeeds(self):
        """Verify no-param methods succeed when params is explicitly null."""
        self.log("Test: no-param methods with params: null")

        resp = self.node.rpc.noraise_request("getblockcount", params=None)
        self.assert_success(resp)

    def test_noparammethods_emptyarray_succeeds(self):
        """Verify no-param methods succeed when params is an empty array."""
        self.log("Test: no-param methods with empty array params")

        resp = self.node.rpc.noraise_request("getblockcount", params=[])
        self.assert_success(resp)

    def test_positionalparams_validargs_succeeds(self):
        """Verify methods accept valid positional (array) parameters."""
        self.log("Test: positional params")

        # getblockhash with positional param: height 0
        resp = self.node.rpc.noraise_request("getblockhash", params=[0])
        self.assert_success(resp)

        genesis_hash = resp["body"]["result"]

        # getblockheader with positional param: genesis hash
        resp = self.node.rpc.noraise_request("getblockheader", params=[genesis_hash])
        self.assert_success(resp)

        # getblock with positional params: hash, verbosity
        resp = self.node.rpc.noraise_request("getblock", params=[genesis_hash, 1])
        self.assert_success(resp)

    def test_namedparams_validargs_succeeds(self):
        """Verify methods accept valid named (object) parameters."""
        self.log("Test: named params")

        resp = self.node.rpc.noraise_request("getblockhash", params={"block_height": 0})
        self.assert_success(resp)

        genesis_hash = resp["body"]["result"]

        resp = self.node.rpc.noraise_request(
            "getblockheader", params={"block_hash": genesis_hash}
        )
        self.assert_success(resp)

        resp = self.node.rpc.noraise_request(
            "getblock", params={"block_hash": genesis_hash, "verbosity": 0}
        )
        self.assert_success(resp)

    def test_optionalparams_omitted_usesdefaults(self):
        """Verify omitted optional parameters fall back to their defaults."""
        self.log("Test: optional defaults")

        genesis_hash = self.node.rpc.get_bestblockhash()

        # getblock with only the required param (verbosity defaults to 1)
        resp_default = self.node.rpc.noraise_request("getblock", params=[genesis_hash])
        self.assert_success(resp_default)

        # Result should be verbose (verbosity=1): an object, not a hex string
        result = resp_default["body"]["result"]
        self.assertIn("hash", result)
        self.assertIn("tx", result)

        # Explicit verbosity=1 should match the default
        resp_explicit = self.node.rpc.noraise_request(
            "getblock", params=[genesis_hash, 1]
        )
        self.assert_success(resp_explicit)
        self.assertEqual(
            resp_default["body"]["result"], resp_explicit["body"]["result"]
        )

        # getmemoryinfo with omitted default
        resp = self.node.rpc.noraise_request("getmemoryinfo")
        self.assert_success(resp)

        # Named params: only required field, optional uses default
        resp = self.node.rpc.noraise_request(
            "getblock", params={"block_hash": genesis_hash}
        )
        self.assert_success(resp)
        self.assertEqual(
            resp_default["body"]["result"], resp_explicit["body"]["result"]
        )
        self.assertIn("hash", resp["body"]["result"])

    def test_unknownmethod_anyparams_returnsmethodnotfound(self):
        """Verify unknown methods return METHOD_NOT_FOUND (-32601)."""
        self.log("Test: method not found")

        resp = self.node.rpc.noraise_request("nonexistent_method", params=[])
        self.assert_error(
            resp, expected_status_code=404, expected_rpcerror_code=METHOD_NOT_FOUND
        )

    def test_requiredparams_missing_returnsinvalidparams(self):
        """Verify missing required parameters return INVALID_PARAMS (-32602)."""
        self.log("Test: missing required params")

        # getblockhash requires a height parameter
        resp = self.node.rpc.noraise_request("getblockhash", params=[])
        self.assert_error(
            resp, expected_status_code=400, expected_rpcerror_code=INVALID_PARAMS
        )

        # getblockheader requires a block_hash, not an int
        resp = self.node.rpc.noraise_request("getblockheader", params=[1])
        self.assert_error(
            resp, expected_status_code=400, expected_rpcerror_code=INVALID_PARAMS
        )

        # Named params: empty object means missing required fields
        resp = self.node.rpc.noraise_request("getblockhash", params={})
        self.assert_error(
            resp, expected_status_code=400, expected_rpcerror_code=INVALID_PARAMS
        )

    def test_paramtypes_wrongtype_returnsinvalidparams(self):
        """Verify wrong parameter types return INVALID_PARAMS (-32602)."""
        self.log("Test: wrong param types")

        # getblockhash expects a number, not a string
        resp = self.node.rpc.noraise_request("getblockhash", params=["not_a_number"])
        self.assert_error(
            resp, expected_status_code=400, expected_rpcerror_code=INVALID_PARAMS
        )

        # getblock expects a valid block hash string, not a number
        resp = self.node.rpc.noraise_request("getblock", params=[12345])
        self.assert_error(
            resp, expected_status_code=400, expected_rpcerror_code=INVALID_PARAMS
        )

        # getblock verbosity expects a number, not a string
        genesis_hash = self.node.rpc.get_bestblockhash()
        resp = self.node.rpc.noraise_request(
            "getblock", params=[genesis_hash, "invalid_verbosity"]
        )
        self.assert_error(
            resp, expected_status_code=400, expected_rpcerror_code=INVALID_PARAMS
        )

    def test_jsonrpcversion_invalid_returnsrejection(self):
        """Verify invalid jsonrpc versions are rejected and valid ones accepted."""
        self.log("Test: invalid jsonrpc version")

        resp = self.node.rpc.noraise_raw_request(
            {
                "jsonrpc": "3.0",
                "id": "test",
                "method": "getblockcount",
                "params": [],
            }
        )
        self.assert_error(
            resp, expected_status_code=400, expected_rpcerror_code=INVALID_REQUEST
        )

        # Valid versions ("1.0" and "2.0") should work
        for version in ["1.0", "2.0"]:
            resp = self.node.rpc.noraise_raw_request(
                {
                    "jsonrpc": version,
                    "id": "test",
                    "method": "getblockcount",
                    "params": [],
                }
            )
            self.assert_success(resp)

        # Omitted jsonrpc field should work (pre-2.0 compat)
        resp = self.node.rpc.noraise_raw_request(
            {
                "id": "test",
                "method": "getblockcount",
            }
        )
        self.assert_success(resp)

    def test_parammethods_omittedparams_returnserror(self):
        """Verify methods that require params fail when params are omitted."""
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
            resp = self.node.rpc.noraise_request(method)
            self.assert_error(
                resp, expected_status_code=400, expected_rpcerror_code=INVALID_PARAMS
            )

    def test_responsestructure_success_matchesjsonrpcspec(self):
        """Verify successful responses match the JSON-RPC 2.0 spec structure."""
        self.log("Test: success response structure")

        resp = self.node.rpc.noraise_raw_request(
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
        self.assertIsSome(body.get("result"))

    def test_responsestructure_error_matchesjsonrpcspec(self):
        """Verify error responses match the JSON-RPC 2.0 spec structure."""
        self.log("Test: error response structure")

        resp = self.node.rpc.noraise_raw_request(
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
        self.assertTrue(isinstance(err["code"], int))
        self.assertEqual(body["id"], "struct_err")


if __name__ == "__main__":
    RpcServerRequestParsingTest().main()
