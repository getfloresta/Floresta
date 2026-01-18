"""
floresta_cli_getmemoryinfo.py

This functional test cli utility to interact with a Floresta node with `getmemoryinfo`
"""

import sys
from test_framework import FlorestaTestFramework, Node, NodeType


class GetMemoryInfoTest(FlorestaTestFramework):
    """
    Test `getmemoryinfo` rpc call, by creating a node and get
    some memory stats with `stats` and `mallocinfo` modes
    """

    def set_test_params(self):
        """Initializes a single Floresta node with default arguments"""
        self.florestad = self.add_node_default_args(variant=NodeType.FLORESTAD)

    def test_mode_stats_ibd(self, node: Node):
        """
        Test `getmemoryinfo stats` when node is in IBD.
        It should return a dictionary with key(str)/value(int).
        """
        if sys.platform == "linux":
            result = node.rpc.get_memoryinfo("stats")
            self.assertIn("locked", result)
            self.assertIn("chunks_free", result["locked"])
            self.assertIn("chunks_used", result["locked"])
            self.assertIn("free", result["locked"])
            self.assertIn("locked", result["locked"])
            self.assertIn("total", result["locked"])
            self.assertIn("used", result["locked"])
            self.assertTrue(result["locked"]["chunks_free"] >= 0)
            self.assertTrue(result["locked"]["chunks_used"] >= 0)
            self.assertTrue(result["locked"]["free"] >= 0)
            self.assertTrue(result["locked"]["locked"] >= 0)
            self.assertTrue(result["locked"]["total"] >= 0)
            self.assertTrue(result["locked"]["used"] >= 0)
        else:
            self.log(
                f"Skipping test: 'getmemoryinfo stats' not implemented for '{sys.platform}'"
            )

    def test_mode_mallocinfo_ibd(self, node):
        """
        Test `getmemoryinfo mallocinfo` when node is in IBD
        It should return a malloc xml string.
        """
        if sys.platform == "linux":
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
            result = node.rpc.get_memoryinfo("mallocinfo")
            self.assertMatch(result, pattern)
        else:
            self.log(
                f"Skipping test: 'getmemoryinfo malloc' not implemented for '{sys.platform}'"
            )

    def run_test(self):
        """
        Run JSONRPC server on first, wait to connect, then call `addnode ip[:port]`
        """
        # Start node
        self.run_node(self.florestad)

        # Test assertions
        self.test_mode_stats_ibd(self.florestad)
        self.test_mode_mallocinfo_ibd(self.florestad)


if __name__ == "__main__":
    GetMemoryInfoTest().main()
