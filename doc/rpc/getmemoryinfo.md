# `getmemoryinfo`

Returns statistics about Floresta's memory usage.

## Usage

### Synopsis

```bash
floresta-cli getmemoryinfo [mode]
```

### Examples

```bash
floresta-cli getmemoryinfo
floresta-cli getmemoryinfo "stats"
```

## Arguments

- `mode` - (string, optional, default="stats") The type of memory information to return. Currently only "stats" is supported.

## Returns

### Ok Response

A JSON object containing memory statistics:

- `used` - (numeric) Currently used memory in bytes.
- `free` - (numeric) Available free memory in bytes.
- `total` - (numeric) Total memory allocated to the process in bytes.
- `locked` - (numeric) Amount of locked (non-swappable) memory in bytes.
- `chunks_used` - (numeric) Number of memory chunks in use.
- `chunks_free` - (numeric) Number of free memory chunks.

### Error Enum `CommandError`

Any of the error types on `rpc_types::Error`.

## Notes

- Returns zeroed values for all runtimes that are not `*-gnu` (glibc) or macOS.
- This is useful for monitoring node resource usage.
- Memory statistics come from the system's memory allocator.
- The `locked` field shows memory protected from swapping (for security-sensitive data).
