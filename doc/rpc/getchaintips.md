# `getchaintips`

Return information about all known tips in the block tree, including the
main chain as well as orphaned branches.

## Usage

### Synopsis

```
floresta-cli getchaintips
```

### Examples

```bash
floresta-cli getchaintips
```

## Arguments

This method takes no arguments.

## Returns

### Ok Response

A JSON array of objects, one per chain tip:

- `height` - (numeric) The height of the chain tip
- `hash` - (string) The block hash of the chain tip, hex-encoded
- `branchlen` - (numeric) Length of the branch connecting the tip to the main chain (0 for the active tip)
- `status` - (string) The validation status of the chain tip. One of:
  * `active` - This is the current best chain tip
  * `valid-fork` - This is a fully validated branch that is not part of the active chain
  * `headers-only` - Headers have been received but blocks are not yet fully validated
  * `invalid` - The branch contains at least one invalid block

## Notes

- There is always exactly one tip with status `active`
- The `active` tip always has `branchlen` equal to 0
- For non-active tips, `branchlen` measures the distance from the fork point to the tip
