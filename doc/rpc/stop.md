# `stop`

Requests a graceful shutdown of the Floresta node.

## Usage

### Synopsis

```bash
floresta-cli stop
```

### Examples

```bash
floresta-cli stop
```

## Arguments

This command takes no arguments.

## Returns

### Ok Response

- (string) Returns the message "Floresta stopping".

### Error Enum `CommandError`

Any of the error types on `rpc_types::Error`.

## Notes

- This initiates a graceful shutdown of the florestad process.
- The node will finish current operations before stopping.
- All data is safely persisted before shutdown.
- Use this instead of forcefully killing the process to avoid data corruption.
