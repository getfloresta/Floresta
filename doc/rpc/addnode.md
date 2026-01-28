# `addnode`

Attempts to add or remove a node from the addnode list.

## Usage

### Synopsis

```bash
floresta-cli addnode <ip:[port]> <add|remove|onetry> [true|false]
```

### Examples

```bash
floresta-cli addnode 192.168.0.1 add true
floresta-cli addnode 192.168.0.1 add false
floresta-cli addnode 192.168.0.1 remove # does not accept the boolean for v2transport
floresta-cli addnode 192.168.0.1 onetry true
floresta-cli addnode 192.168.0.1 onetry false
```


## Arguments

`node` - (string, required) The <IP:[PORT]> address of the peer to connect to.

`command` - (string, required) 'add' to add a node to the list, 'remove' to remove a node from the list, 'onetry' to try a connection to the node once.

`v2transport` - (boolean, optional) Only tries to connect with this address using BIP0324 P2P V2 protocol. ignored for 'remove' command.

## Returns

### Ok response

- json null

### Error response

- `InvalidAddress` - The provided address is not a valid socket address or IP address
- `InvalidAddnodeCommand` - The command is not one of 'add', 'remove', or 'onetry'

## Notes

- Will use v2transport if available unless the param is specified to `false`
- Implementation detail for `addnode`: on bitcoin-core, the node doesn't connect immediately after adding a peer, it just adds it to the `added_peers` list. Here we do almost the same, but we do an early connection attempt to the peer, so we can start communicating with.
