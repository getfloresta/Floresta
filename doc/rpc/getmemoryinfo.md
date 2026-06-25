# `getmemoryinfo`

Returns statistics about Floresta's memory usage.

## Usage

### Synopsis

```bash
floresta-cli getmemoryinfo <mode>
```

### Examples

```bash
# Get memory usage statistics (default mode)
floresta-cli getmemoryinfo

# Get the same information, explicitly
floresta-cli getmemoryinfo stats

# Get a raw XML dump of the allocator statistics
floresta-cli getmemoryinfo mallocinfo
```

## Arguments

`mode` - (string, optional, default=`stats`) Determines the format of the returned information.

  * `stats` - Returns a structured breakdown of memory usage.

  * `mallocinfo` - Returns a raw XML string with the allocator statistics, in the same format as glibc's `malloc_info`.

## Returns

### Ok Response

If `mode` is `stats`:

* `locked` - (json object)
  * `used` - (numeric) Memory currently in use, in bytes;
  * `free` - (numeric) Memory currently free, in bytes;
  * `total` - (numeric) Total memory allocated, in bytes;
  * `locked` - (numeric) Total memory locked, in bytes;
  * `chunks_used` - (numeric) How many chunks are currently in use;
  * `chunks_free` - (numeric) How many chunks are currently free.

If `mode` is `mallocinfo`:

* (string) A raw XML string with the allocator statistics.

### Error Enum `CommandError`

* `JsonRpcError::InvalidMemInfoMode`

## Notes

* This command relies on the system allocator, and is only implemented for `*-gnu` Linux targets and macOS. On any other runtime, all numeric fields are returned as zero.
* Any `mode` value other than `stats` or `mallocinfo` results in an error.
