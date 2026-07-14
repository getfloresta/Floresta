# SPDX-License-Identifier: MIT OR Apache-2.0

"""Utility helpers used by the test framework (paths, ports, TLS helpers)."""

import os
import time
import inspect
import random
import socket
import subprocess
import math

from test_framework.crypto.pkcs8 import (
    create_pkcs8_private_key,
    create_pkcs8_self_signed_certificate,
)

SERVICE_FLAGS_BY_NAME = {
    "NETWORK": 1 << 0,
    "GETUTXO": 1 << 1,
    "BLOOM": 1 << 2,
    "WITNESS": 1 << 3,
    "COMPACT_FILTERS": 1 << 6,
    "NETWORK_LIMITED": 1 << 10,
    "P2P_V2": 1 << 11,
    "UTREEXO": 1 << 12,
    "UTREEXO_ARCHIVE": 1 << 13,
}

BITCOIND_TEST_FRAMEWORK_SERVICES = "0000000000000c09"
BITCOIND_TEST_FRAMEWORK_SERVICESNAMES = {
    "NETWORK",
    "WITNESS",
    "NETWORK_LIMITED",
    "P2P_V2",
}


class Utility:
    """
    A utility class for common functions used in the test framework.
    """

    @staticmethod
    def get_integration_test_dir():
        """
        Get path for florestad used in integration tests, generally set on
        $FLORESTA_TEMP_DIR/binaries
        """
        if os.getenv("FLORESTA_TEMP_DIR") is None:
            raise RuntimeError(
                "FLORESTA_TEMP_DIR not set. "
                + " Please set it to the path of the integration test directory."
            )
        return os.getenv("FLORESTA_TEMP_DIR")

    @staticmethod
    def get_git_describe():
        """
        Get the output of 'git describe --tags --always' command.
        """
        try:
            git_describe = subprocess.check_output(
                ["git", "describe", "--tags", "--always"], text=True
            ).strip()
        except subprocess.CalledProcessError as exc:
            raise RuntimeError(
                "Failed to run 'git describe'. Run this at the Floresta directory."
            ) from exc

        return git_describe

    @staticmethod
    def get_available_random_port_by_range(start: int, end: int):
        """Get an available random port in the range [start, end]"""
        while True:
            port = random.randint(start, end)
            with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
                # Check if the port is available
                if s.connect_ex(("127.0.0.1", port)) != 0:
                    return port

    @staticmethod
    def get_random_port():
        """Get a random port in the range [2000, 65535]"""
        return Utility.get_available_random_port_by_range(2000, 65535)

    @staticmethod
    def create_tls_key_cert() -> tuple[str, str]:
        """
        Create a PKCS#8 formatted private key and a self-signed certificate.
        These keys are intended to be used with florestad's --tls-key-path and --tls-cert-path
        options.
        """
        # If we're in CI, we need to use the
        # path to the integration test dir
        # tempfile will be used to get the proper
        # temp dir for the OS
        tls_rel_path = os.path.join(Utility.get_integration_test_dir(), "data", "tls")
        tls_path = os.path.normpath(os.path.abspath(tls_rel_path))

        # Create the folder if not exists
        os.makedirs(tls_path, exist_ok=True)

        # Create certificates
        pk_path, private_key = create_pkcs8_private_key(tls_path)

        cert_path = create_pkcs8_self_signed_certificate(
            tls_path, private_key, common_name="florestad", validity_days=365
        )

        return (pk_path, cert_path)


def wait_until_helper_internal(
    predicate, *, timeout=60, lock=None, timeout_factor=1.0, check_interval=0.05
):
    """Sleep until the predicate resolves to be True.

    Warning: Note that this method is not recommended to be used in tests as it is
    not aware of the context of the test framework. Using the `wait_until()` members
    from `BitcoinTestFramework` or `P2PInterface` class ensures the timeout is
    properly scaled. Furthermore, `wait_until()` from `P2PInterface` class in
    `p2p.py` has a preset lock.
    """
    timeout = timeout * timeout_factor
    time_end = time.time() + timeout

    while time.time() < time_end:
        if lock:
            with lock:
                if predicate():
                    return
        else:
            if predicate():
                return
        time.sleep(check_interval)

    # Print the cause of the timeout
    predicate_source = "''''\n" + inspect.getsource(predicate) + "'''"
    print(f"wait_until() failed. Predicate: {predicate_source}")
    raise AssertionError(
        f"Predicate {predicate_source} not true after {timeout} seconds"
    )


def wait_until(predicate, timeout=30, interval=0.5, error_msg="Condition not met"):
    """
    Wait until a predicate returns True or timeout is reached.
    """
    start = time.time()
    while time.time() - start < timeout:
        if predicate():
            return True
        time.sleep(interval)

    raise TimeoutError(f"{error_msg} after {timeout} seconds")


def assert_service_fields_consistent(peer_info):
    """
    Assert that getpeerinfo's services and servicesnames describe the same flags.
    """
    services = peer_info["services"]
    assert isinstance(services, str)
    assert len(services) == 16
    services_bits = int(services, 16)

    services_names = peer_info["servicesnames"]
    assert isinstance(services_names, list)
    assert all(isinstance(name, str) for name in services_names)

    services_names = set(services_names)
    assert services_names <= set(SERVICE_FLAGS_BY_NAME)

    for name, flag in SERVICE_FLAGS_BY_NAME.items():
        assert bool(services_bits & flag) == (name in services_names), name


def assert_bitcoind_service_fields(peer_info):
    """
    Assert getpeerinfo service fields for the Bitcoin Core test framework node.
    """
    assert_service_fields_consistent(peer_info)
    assert peer_info["services"] == BITCOIND_TEST_FRAMEWORK_SERVICES
    assert set(peer_info["servicesnames"]) == BITCOIND_TEST_FRAMEWORK_SERVICESNAMES


def compare_fields(
    candidate, reference, ignore_fields=None, float_tol=1e-8, ordered_lists=True
):
    """
    Recursively compare two data structures (dicts, lists, or scalars),
    ignoring specified fields.

    Note:
        The comparison is asymmetric. `reference` defines the required fields.
        For Floresta RPC tests, use Floresta as `candidate` and the reference
        node as `reference`.

    Args:
        ordered_lists: If True (default), lists are compared element-by-element
        in order. If False, scalar items (non-dict, non-list, non-float) are
        only checked for presence in the candidate list, regardless of order.
    """
    if ignore_fields is None:
        ignore_fields = set()
    elif isinstance(ignore_fields, list):
        ignore_fields = set(ignore_fields)

    # float tolerance
    if isinstance(candidate, float) or isinstance(reference, float):
        assert math.isclose(candidate, reference, rel_tol=0.0, abs_tol=float_tol), (
            f"Float mismatch: candidate={candidate}, reference={reference}, "
            f"tolerance={float_tol}"
        )
        return

    # dict
    if isinstance(candidate, dict) and isinstance(reference, dict):
        for key, ref_value in reference.items():
            if key in ignore_fields:
                continue
            assert key in candidate, f"Missing key in candidate: {key}"
            compare_fields(
                candidate[key],
                ref_value,
                ignore_fields=ignore_fields,
                float_tol=float_tol,
                ordered_lists=ordered_lists,
            )
        return

    # list
    if isinstance(candidate, list) and isinstance(reference, list):
        if not ordered_lists:
            for ref_item in reference:
                if isinstance(ref_item, (dict, list, float)):
                    # complex types: fall back to ordered comparison by position
                    compare_fields(
                        candidate[reference.index(ref_item)],
                        ref_item,
                        ignore_fields=ignore_fields,
                        float_tol=float_tol,
                        ordered_lists=ordered_lists,
                    )
                else:
                    assert (
                        ref_item in candidate
                    ), f"Item {ref_item!r} not found in candidate list {candidate!r}"
            return

        assert len(candidate) == len(
            reference
        ), f"List length mismatch: expected {len(reference)}, got {len(candidate)}"
        for cand_item, ref_item in zip(candidate, reference):
            compare_fields(
                cand_item,
                ref_item,
                ignore_fields=ignore_fields,
                float_tol=float_tol,
                ordered_lists=ordered_lists,
            )
        return

    # scalar
    assert candidate == reference
