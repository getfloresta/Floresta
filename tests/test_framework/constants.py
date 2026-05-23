# SPDX-License-Identifier: MIT OR Apache-2.0

"""
This module contains constants used throughout the Floresta tests.
"""

import os

# defaults to import...
GENESIS_BLOCK_HEIGHT = 0
GENESIS_BLOCK_HASH = "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
GENESIS_BLOCK_DIFFICULTY_INT = 1
GENESIS_BLOCK_DIFFICULTY_FLOAT = 4.656542373906925e-10
GENESIS_BLOCK_LEAF_COUNT = 0
CHAIN_NAME = "regtest"
FLORESTA_TEMP_DIR = os.getenv("FLORESTA_TEMP_DIR")

# Wallets information,
# Mnemonics = useless ritual arm slow mention dog force almost sudden pulp rude eager
# pylint: disable = line-too-long
WALLET_XPRIV = "tprv8hCwaWbnCTeqSXMmEgtYqC3tjCHQTKphfXBG5MfWgcA6pif3fAUqCuqwphSyXmVFhd8b5ep5krkRxF6YkuQfxSAhHMTGeRA8rKPzQd9BMre"
WALLET_DESCRIPTOR_PRIV_INTERNAL = f"wpkh({WALLET_XPRIV}/1/*)#v08p3aj4"
WALLET_DESCRIPTOR_PRIV_EXTERNAL = f"wpkh({WALLET_XPRIV}/0/*)#amzqvgzd"
# pylint: disable = line-too-long
WALLET_XPUB = "tpubDDtyive2LqLWKzPZ8LZ9Ebi1JDoLcf1cEpn3Mshp6sxVfCupHZJRPQTozp2EpTF76vJcyQBN7VP7CjUntEJxeADnuTMNTYKoSWNae8soVyv"
WALLET_DESCRIPTOR_INTERNAL = f"wpkh({WALLET_XPUB}/1/*)#0rlhs7rw"
WALLET_DESCRIPTOR_EXTERNAL = f"wpkh({WALLET_XPUB}/0/*)#7h6kdtnk"
# pylint: disable = line-too-long
WALLET_XPUB_BIP_84 = "vpub5ZrpbMUWLCJ6MbpU1RzocWBddAQnk2XYry9JSXrtzxSqoicei28CzqUhiN2HJ8z2VjY6rsUNf4qxjym43ydhAFQJ7BDDcC2bK6et6x9hc4D"
WALLET_ADDRESS = "bcrt1q427ze5mrzqupzyfmqsx9gxh7xav538yk2j4cft"

# Legacy (P2PKH) descriptors derived from the same xpriv/xpub above.
# Used in tests that need to spend coinbases without segwit activation.
WALLET_DESCRIPTOR_PRIV_EXTERNAL_PKH = f"pkh({WALLET_XPRIV}/0/*)#e7wql9sy"
WALLET_DESCRIPTOR_EXTERNAL_PKH = f"pkh({WALLET_XPUB}/0/*)#kdd2p3pr"
WALLET_ADDRESS_PKH = "mw5ibJNmFmX9bEjZwr8Lo3YZT15XY16vpB"

# JSON-RPC spec error code constants
JSONRPC_ERRCODE_PARSE = -32700
JSONRPC_ERRCODE_INVALID_REQUEST = -32600
JSONRPC_ERRCODE_METHOD_NOT_FOUND = -32601
JSONRPC_ERRCODE_INVALID_PARAMS = -32602
JSONRPC_ERRCODE_INTERNAL = -32603

# Floresta-specific server error codes (-32099..=-32000)
JSONRPC_ERRCODE_BLOCK_NOT_FOUND = -32098

# JSON-RPC error message constants
JSONRPC_ERRMSG_MISSING_PARAMS = "Missing parameter"
JSONRPC_ERRMSG_WRONG_PARAM_TYPE = "Invalid parameter type"
JSONRPC_ERRMSG_METHOD_NOT_FOUND = "Method not found"
JSONRPC_ERRMSG_INVALID_VERSION = "The request contains a invalid jsonrpc version"
JSONRPC_ERRMSG_MALFORMATED_PARAMS = (
    "A parameter is malformated, the parameter MUST be an array or an object"
)

# RPC method lists for testing
NO_PARAM_METHODS = [
    "getbestblockhash",
    "getblockchaininfo",
    "getblockcount",
    "getroots",
    "getrpcinfo",
    "uptime",
    "getpeerinfo",
    "listdescriptors",
]

METHODS_REQUIRING_PARAMS = [
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
