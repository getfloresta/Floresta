# Invalid block candidate 783426

This is F2Pool's non-canonical mainnet block candidate for height 783,426,
`00000000000000000002ec935e245f8ae70fc68cc828f05bf4cfa002668599e4`.
It has valid proof of work, but Bitcoin Core rejected its 80,003 sigop cost as
`bad-blk-sigops`; the consensus maximum is 80,000. See
[b10c's analysis](https://b10c.me/observations/11-invalid-blocks-783426-and-784121/).

`raw.zst` contains the serialized block, while `spent_utxos.zst` contains the
ordered spent-output metadata needed for contextual validation.
