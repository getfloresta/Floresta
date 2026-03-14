# `submitheader`

Decodes the given hex data as a block header and submits it as a candidate chain tip if valid. The header is accepted but the full block data is not required.

## Usage

### Synopsis

```bash
floresta-cli submitheader <hexdata>
```

### Examples

```bash
# Submit a raw 80-byte block header in hex (160 hex characters)
floresta-cli submitheader "0000002006226e46111a0b59caaf126043eb5bbf28c34f3a5e332a1fc7b2b73cf188910f..."
```

## Arguments

`hexdata` - (string, required) The hex-encoded block header data (160 hex characters representing 80 bytes).

## Returns

### Ok Response

Returns `null` on success.

### Error Enum `CommandError`

- `JsonRpcError::InvalidHex` - If the provided hex string is not valid hexadecimal.
- `JsonRpcError::Decode` - If the hex data cannot be deserialized as a block header.
- `JsonRpcError::Chain` - If the header is invalid (e.g., unknown previous block, invalid proof of work).

## Notes

- The header's `prev_blockhash` must reference a block already known to the node.
- On success the node's tip advances but the block is stored as `HeadersOnly`, itll become `FullyValid` after validating its transactions.
- This is useful for testing header-first sync scenarios without providing full block data.
