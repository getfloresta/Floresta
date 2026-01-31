"""
Test the `verifyutxochaintipinclusionproof` RPC call.
"""

from test_framework import FlorestaTestFramework
from requests.exceptions import HTTPError


class VerifyUtxoChainTipInclusionProofTest(FlorestaTestFramework):
    """
    Test `verifyutxochaintipinclusionproof` RPC is accessible and handles
    basic input validation.
    """

    def set_test_params(self):
        """Setup a single florestad node"""
        self.florestad = self.add_node(variant="florestad")

    def run_test(self):
        """Test RPC accessibility and basic validation"""
        self.run_node(self.florestad)

        # RPC rejects invalid hex input
        with self.assertRaises(HTTPError):
            self.florestad.rpc.verifyutxochaintipinclusionproof("invalid_hex")


if __name__ == "__main__":
    VerifyUtxoChainTipInclusionProofTest().main()
