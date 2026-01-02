"""
floresta_cli_getmemoryinfo.py

This functional test cli utility to interact with a Floresta node with `getmemoryinfo`
"""

import sys
import re
import pytest


@pytest.mark.rpc
def test_get_memory_info(setup_logging, florestad_node):
    """Test `getmemoryinfo` rpc call."""
    log = setup_logging
    if sys.platform != "linux":
        log.info(f"Skipping test: 'getmemoryinfo' not implemented for '{sys.platform}'")
        return

    florestad = florestad_node

    log.info("Testing 'getmemoryinfo' rpc call stats")
    result = florestad.rpc.get_memoryinfo("stats")
    log.info(f"Memory info stats: {result}")
    assert result is not None
    assert isinstance(result, dict)

    memory_info = result["locked"]
    expected_keys = ["locked", "chunks_free", "chunks_used", "free", "total", "used"]
    for key in expected_keys:
        value = memory_info[key]
        log.debug(f"Checking key '{key}' and value '{value}'")
        assert key in memory_info
        assert value >= 0

    pattern = (
        r'<malloc version="[^"]+">'
        r'<heap nr="\d+">'
        r"<allocated>\d+</allocated>"
        r"<free>\d+</free>"
        r"<total>\d+</total>"
        r"<locked>\d+</locked>"
        r'<chunks nr="\d+">'
        r"<used>\d+</used>"
        r"<free>\d+</free>"
        r"</chunks>"
        r"</heap>"
        r"</malloc>"
    )
    result = florestad.rpc.get_memoryinfo("mallocinfo")

    assert re.fullmatch(pattern, result)
