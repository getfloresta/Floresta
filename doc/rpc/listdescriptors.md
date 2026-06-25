# `listdescriptors`

Returns a list of all descriptors currently loaded in the watch-only wallet.

## Usage

### Synopsis

```bash
floresta-cli listdescriptors
```

### Examples

```bash
# List every descriptor currently tracked by the wallet
floresta-cli listdescriptors
```

## Arguments

This command takes no arguments.

## Returns

### Ok Response

Returns a JSON array of strings, each one a descriptor currently loaded in the wallet.

### Error Enum `CommandError`

* `JsonRpcError::Wallet`

## Notes

* Descriptors are added with [`loaddescriptor`](loaddescriptor.md). This command lets you confirm which ones are currently active before deciding whether to load more.
* Returns an empty array if no descriptor has been loaded yet.
