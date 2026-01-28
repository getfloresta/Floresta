# `command_name`

Brief description of what this RPC does and its primary purpose.

## Usage

### Synopsis

```
floresta-cli <command> <param1, parameter_type> <param2> [<optional_flag, (-f or --flag)>]
```

### Examples

```bash
floresta-cli templatecommand -f "data"
floresta-cli templatecommand --flag "moredata"
floresta-cli templatecommand  123
```

## Arguments

`param1` - (type, required or optional) Description of the first parameter

  * `json_string` (string, required) Description about the expected JSON object.

  * `numeric_field` (numeric, optional) Describes an optional field.

  * `boolean_field`(boolean, required) Describes an obligatory boolean.


`param2`  - (type, required or optional) Description of optional parameter 

## Returns

### Ok Response

- `field1`- (type) Description of return field
- `field2` - (type) Description of another return field

### Error Enum `CommandError`

(Command error is a well documented enum returned client side)

## Notes

- Any important behavioral notes or requirements
- Performance considerations if applicable
- Related RPC methods or alternatives
