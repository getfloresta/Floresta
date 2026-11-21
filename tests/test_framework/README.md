## Floresta test framework

This is a collection of tools to help with functional testing of Floresta. The actual tests are written in Python and can be found in the `tests` directory.

Some good chunks of this framework are taken from [Bitcoin Core's test framework](https://github.com/bitcoin/bitcoin/tree/master/test/functional) and adapted to Floresta. Luckily, both are MIT licensed, so we just left the original license headers in place. Thanks to the Bitcoin Core developers for their work!

### Main components

 - `test_framework.test_framework`: Base class for tests, helps you with setting up a test node and provides some useful methods. This also implements the main test workflow.
 - `bitcoin`: Useful bitcoin primitives, like transactions, blocks, etc.
 - `electrum_client`: A simple client for the Electrum protocol, we use it to test our own Electrum server implementation.
 - `key`: Useful key primitives, like BIP32 derivation, signing, etc.
 - `secp256k1`: A simple implementation of the secp256k1 elliptic curve, used for signing and verification.
 - `utreexod`: A wrapper around `utreexod`. It helps you with starting and stopping a `utreexod` instance, and provides some useful methods to interact with it. Should if we need a utreexo peer during a test.
 `utreexo`: Useful utreexo methods, like creating proofs.

### Running the tests

To run the tests, you need to have a `utreexod` instance running in the background. You can start it with `./tests/utreexod.py`. It will create a new datadir in `/tmp/utreexod` and start a `utreexod` instance in regtest mode. It will also create a new wallet for you, and fund it with 50 BTC. The wallet is encrypted with the password `test`.

You can run the tests with `./test_runner.py`. It will start a new `utreexod` instance for each test, and stop it afterwards. You can also run a single test with `./test_runner.py <test_name>`. You can get a list of all tests with `./test_runner.py --list`.

### Writing tests

To write a new test, create a new file in the `tests` directory. The file name should start with `test_`. The file should contain a class that inherits from `TestFramework`. You can then implement the `set_test_params` and `run_test` methods. The `set_test_params` method is called before the test is run, and you can use it to set some test parameters, like the number of nodes to start, or the number of blocks to mine. The `run_test` method is called after the nodes are started, and you can use it to actually run the test.

You can use the `self.nodes` list to access the nodes. The first node in the list is the controller node, and the other nodes are the peers. You can use the `self.nodes[0].rpc` attribute to access the RPC interface of the controller node. You can use the `self.nodes[0].utreexo` attribute to access the utreexo interface of the controller node. You can use the `self.nodes[0].electrum` attribute to access the Electrum interface of the controller node.

You can use the `self.log` method to log messages. The log messages will be prefixed with the test name.