# `listdescriptors`

Returns a list of all descriptors currently loaded in the watch-only wallet.

## Usage

### Synopsis

```bash
floresta-cli listdescriptors
```

### Examples

```bash
floresta-cli listdescriptors
```

## Arguments

This command takes no arguments.

## Returns

### Ok Response

- (json array of strings) An array of descriptor strings loaded in the wallet.

```json
[
  "wpkh([fingerprint/84'/0'/0']xpub.../0/*)#checksum",
  "wpkh([fingerprint/84'/0'/0']xpub.../1/*)#checksum"
]
```

### Error Enum `CommandError`

Any of the error types on `rpc_types::Error`.

## Notes

- Descriptors define which addresses the wallet watches.
- Use `loaddescriptor` to add new descriptors to the wallet.
- Common descriptor types include:
  - `wpkh` - Native SegWit (P2WPKH)
  - `pkh` - Legacy (P2PKH)
  - `sh(wpkh())` - Wrapped SegWit (P2SH-P2WPKH)
- The checksum (after `#`) ensures descriptor integrity.
- Descriptors persist across node restarts.
