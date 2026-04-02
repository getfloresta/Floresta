# `sendrawtransaction`

Broadcast a serialized transaction to the P2P network.

## Usage

### Synopsis

```bash
floresta-cli sendrawtransaction <serialized transaction>
```

### Examples

```bash
floresta-cli sendrawtransaction 02000000000101d536437a10d4d22c471e5b471a12b899a029b4827b10c1a6c35c5c878595e1860000000000fdffffff014d320400000000001600147a10b51654c098124d8e28663c8ac1f90ea9cf3102473044022038263d9c36865a2699956b21bdb862f46aecabb8d22855063d64070f7bf9968d02203a753b9b3844656d126e30122a1ed6db2289c9eca4326500097e6e9c8e86f659012102649b8a79c4e084416a8c89075e47325bf432a6982f2ada546ba4fa457e9ba036ea6f0400
```

## Arguments

`tx` - (string, required) The serialized transaction

## Returns

### Ok Response

- The transaction's txid.

### Error

- `InvalidHex` - The hex string is malformed
- `Decode` - The hex string could not be parsed into a transaction
- `AlreadyKnown` - The transaction is already in the mempool
- `MemoryUsageTooHigh` - The mempool is full and cannot currently accept more transactions
- `ConflictingTransaction` - The transaction is conflicting with another transaction in the mempool
- `DuplicatedInputs` - The transaction has duplicated inputs
- `FeeTooLow` - The transaction does not meet the minimum relay feerate
- `ExceedsMaxWeight` - The transaction exceeds the maximum standard relay weight
- `ExceedsScriptSigSize` - The transaction exceeds the standard `scriptSig` size policy
- `NonStandard` - The transaction is non-standard under relay policy
- `MissingPrevoutContext` - The node could not resolve enough prevout metadata to evaluate relay policy
- `Consensus` - The transaction failed consensus checks
