# `getdifficulty`

Returns the proof-of-work difficulty as a multiple of the minimum difficulty.

## Usage

### Synopsis

floresta-cli getdifficulty

### Examples

```bash
floresta-cli getdifficulty
```

## Returns

### Ok response

- `difficulty` - (numeric) The proof-of-work difficulty as a multiple of the minimum difficulty.

## Notes

- The returned value may differ from `bitcoin-cli getdifficulty` at the last
  digits of the f64 mantissa due to floating-point rounding in rust-bitcoin's
  implementation. The difference is on the order of machine epsilon (~1e-16)
  and has no practical effect.
