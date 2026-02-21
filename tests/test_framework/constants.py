"""
This module contains constants used throughout the Floresta tests.
"""

import os

GENESIS_BLOCK_HEIGHT = 0
GENESIS_BLOCK_HASH = "0f9188f13cb7b2c71f2a335e3a4fc328bf5beb436012afca590b1a11466e2206"
GENESIS_BLOCK_DIFFICULTY_INT = 1
GENESIS_BLOCK_DIFFICULTY_FLOAT = 4.656542373906925e-10
GENESIS_BLOCK_LEAF_COUNT = 0
TEST_CHAIN = "regtest"
FLORESTA_TEMP_DIR = os.getenv("FLORESTA_TEMP_DIR")

# Mnemonics = useless ritual arm slow mention dog force almost sudden pulp rude eager
# pylint: disable = line-too-long
WALLET_XPRIV = "tprv8hCwaWbnCTeqSXMmEgtYqC3tjCHQTKphfXBG5MfWgcA6pif3fAUqCuqwphSyXmVFhd8b5ep5krkRxF6YkuQfxSAhHMTGeRA8rKPzQd9BMre"
WALLET_DESCRIPTOR_PRIV_EXTERNAL = f"wpkh({WALLET_XPRIV}/0/*)#amzqvgzd"
WALLET_DESCRIPTOR_PRIV_INTERNAL = f"wpkh({WALLET_XPRIV}/1/*)#v08p3aj4"
# pylint: disable = line-too-long
WALLET_XPUB = "tpubDDtyive2LqLWKzPZ8LZ9Ebi1JDoLcf1cEpn3Mshp6sxVfCupHZJRPQTozp2EpTF76vJcyQBN7VP7CjUntEJxeADnuTMNTYKoSWNae8soVyv"
WALLET_DESCRIPTOR_EXTERNAL = f"wpkh({WALLET_XPUB}/0/*)#7h6kdtnk"
WALLET_DESCRIPTOR_INTERNAL = f"wpkh({WALLET_XPUB}/1/*)#0rlhs7rw"
# pylint: disable = line-too-long
WALLET_XPUB_BIP_84 = "vpub5ZrpbMUWLCJ6MbpU1RzocWBddAQnk2XYry9JSXrtzxSqoicei28CzqUhiN2HJ8z2VjY6rsUNf4qxjym43ydhAFQJ7BDDcC2bK6et6x9hc4D"
WALLET_ADDRESS = "bcrt1q427ze5mrzqupzyfmqsx9gxh7xav538yk2j4cft"
