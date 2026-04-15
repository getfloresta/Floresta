# SPDX-License-Identifier: MIT OR Apache-2.0

"""
floresta_cli_getdeploymentinfo.py

Functional test for `getdeploymentinfo`. Mines blocks via utreexod, then
compares florestad's response against bitcoind's for each buried deployment.

Floresta currently emits only buried deployments (bip34, bip66, bip65, csv,
segwit). Bitcoin Core also emits BIP9 deployments (taproot, testdummy) which
require the versionbits state machine; those keys are present in bitcoind's
response but skipped on the floresta side.
"""

import pytest

from test_framework.util import wait_for_chain_sync

TIMEOUT_SECONDS = 30
MINE_BLOCKS = 10

# Five buried deployments per Bitcoin Core's deploymentinfo.cpp enum
# (DEPLOYMENT_HEIGHTINCB, DEPLOYMENT_DERSIG, DEPLOYMENT_CLTV, DEPLOYMENT_CSV,
# DEPLOYMENT_SEGWIT). Anything else in bitcoind's output is BIP9 and intentionally
# absent on floresta.
BURIED_DEPLOYMENTS = ("bip34", "bip66", "bip65", "csv", "segwit")


# pylint: disable=too-many-locals
@pytest.mark.rpc
def test_get_deployment_info(florestad_bitcoind_utreexod_with_chain):
    """
    Compare florestad's getdeploymentinfo response against bitcoind's after a
    small chain extension. Each buried deployment must match by type, height
    and active flag. BIP9 entries are validated by absence on the floresta side.
    """
    florestad, bitcoind, utreexod = florestad_bitcoind_utreexod_with_chain(MINE_BLOCKS)

    wait_for_chain_sync(florestad, bitcoind, utreexod, MINE_BLOCKS, TIMEOUT_SECONDS)

    floresta_info = florestad.rpc.get_deployment_info()
    bitcoind_info = bitcoind.rpc.get_deployment_info()

    # Top-level fields: hash and height must match.
    assert floresta_info["hash"] == bitcoind_info["hash"], (
        f"hash mismatch: floresta={floresta_info['hash']} "
        f"bitcoind={bitcoind_info['hash']}"
    )
    assert floresta_info["height"] == bitcoind_info["height"], (
        f"height mismatch: floresta={floresta_info['height']} "
        f"bitcoind={bitcoind_info['height']}"
    )

    floresta_deps = floresta_info["deployments"]
    bitcoind_deps = bitcoind_info["deployments"]

    # Every buried deployment bitcoind emits must also be in floresta's output,
    # with matching type, height and active flag.
    for name in BURIED_DEPLOYMENTS:
        assert name in bitcoind_deps, (
            f"bitcoind unexpectedly missing buried deployment '{name}': "
            f"{bitcoind_deps.keys()}"
        )
        assert name in floresta_deps, f"floresta missing buried deployment '{name}'"

        b_entry = bitcoind_deps[name]
        f_entry = floresta_deps[name]

        assert (
            f_entry["type"] == "buried"
        ), f"floresta did not classify '{name}' as buried: {f_entry['type']}"
        assert (
            f_entry["height"] == b_entry["height"]
        ), f"{name}: floresta height {f_entry['height']} != bitcoind {b_entry['height']}"
        assert (
            f_entry["active"] == b_entry["active"]
        ), f"{name}: floresta active {f_entry['active']} != bitcoind {b_entry['active']}"

    # Sanity check: floresta should not be emitting any deployment outside the
    # buried set yet (no BIP9 state machine).
    extra = set(floresta_deps.keys()) - set(BURIED_DEPLOYMENTS)
    assert not extra, f"floresta emitted unexpected non-buried deployments: {extra}"

    # Pre-activation contract: at genesis, deployments with height > 0 must be inactive.
    genesis_hash = florestad.rpc.get_blockhash(0)
    floresta_genesis = florestad.rpc.get_deployment_info(genesis_hash)

    assert floresta_genesis["height"] == 0
    for name in BURIED_DEPLOYMENTS:
        f_entry = floresta_genesis["deployments"][name]
        expected_active = f_entry["height"] <= 0
        assert (
            f_entry["active"] == expected_active
        ), f"genesis {name}: active={f_entry['active']} but height={f_entry['height']}"

    # Mid-chain blockhash: exercises the `Some(blockhash)` branch of the handler.
    # Activation heights are network constants already checked at the tip, so
    # only the active flag is compared here.
    mid_height = MINE_BLOCKS // 2
    mid_hash = florestad.rpc.get_blockhash(mid_height)
    floresta_mid = florestad.rpc.get_deployment_info(mid_hash)
    bitcoind_mid = bitcoind.rpc.get_deployment_info(mid_hash)

    assert floresta_mid["height"] == bitcoind_mid["height"] == mid_height
    assert floresta_mid["hash"] == bitcoind_mid["hash"]

    for name in BURIED_DEPLOYMENTS:
        f_entry = floresta_mid["deployments"][name]
        b_entry = bitcoind_mid["deployments"][name]
        assert (
            f_entry["active"] == b_entry["active"]
        ), f"mid {name}: floresta active {f_entry['active']} != bitcoind {b_entry['active']}"
