# SabVM

The Sablier Virtual Machine (SabVM) is a fork of [REVM][revm] with Native Tokens implemented as precompiles. Native Tokens are like ERC-20, but their balances is tracked by the VM state instead of the contract storage. This allows for more efficient token transfers and interactions.

SabVM was part of the [Sablier Mainnet](https://x.com/PaulRBerg/status/1852392802933715061) project, which was discontinued in October 2024. You can read the post-mortem [here](https://x.com/PaulRBerg/status/1852392802933715061).

## Related

1. [EIP-7809](https://github.com/ethereum/EIPs/pull/9026)
2. The README in [REVM][revm]

[revm]: https://github.com/bluealloy/revm