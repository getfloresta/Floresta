"""
restart.py

A simple test that restarts a Floresta node and ensures that the node can
successfully restart using the same data directory.

The test verifies that the node can stop and restart without encountering
issues, such as data corruption or failure to initialize.
"""

from test_framework import FlorestaTestFramework, NodeType


class TestRestart(FlorestaTestFramework):
    """
    Test the restart of a Floresta node using the same data directory.
    Ensures that the node can stop and restart without issues.
    """

    def set_test_params(self):
        """
        Here we define setup for test
        """

        self.floresta = self.add_node_default_args(
            variant=NodeType.FLORESTAD,
        )

    def run_test(self):
        """
        Tests the node's ability to restart without initialization issues.
        This would have caught, the error fixed in #9
        """

        self.run_node(self.floresta)
        self.floresta.stop()

        self.run_node(self.floresta)


if __name__ == "__main__":
    TestRestart().main()
