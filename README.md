## Structs, the data structure service
_Structs_ is a tool for interacting with structured data in shell scripts. Structs allows you to parse some JSON, maintain the data structure in memory, and arbitrarily access fields in a natural way. There are other ways to accomplish this in shell scripts, but they are generally not great.

We do this by running a small service in the background which is automatically started in response to the first Structs command. Subsequent comands will use the same service, and therefore can access the same data. This service automatically exits after there is no more activity for a while—usually a minut for a while—usually a minute.

### Define a data structure
A data structure can be defined via the `set` operation. The key for the newly-created structure is printed and can be used to fetch data.

```sh
$ structs set
{
  "numbers": {
    "one": {
        "cardinal": 1,
        "ordinal": "1st"
    },
    "two": {
        "cardinal": 2,
        "ordinal": "2nd"
    },
    "three": {
        "cardinal": 3,
        "ordinal": "3rd"
    }
  }
}
^D
woh7iu3tieB0
```

### Fetch a structure
The entire data structure can be fetched using its key, or its fields can be refernced using common dot-notation.
```sh
$ structs get woh7iu3tieB0
{"numbers":{"one":{"cardinal":1,"ordinal":"1st"},"two":{"cardinal":2,"ordinal":"2nd"},"three":{"cardinal":3,"ordinal":"3rd"}}}
```

### Fetch a sub-structure
Referencing a field will print a subset of the structure as JSON.
```sh
$ structs get woh7iu3tieB0.numbers.two
{"cardinal":2,"ordinal":"2nd"}
```

Structs can be used in collaboration with [Jq](https://jqlang.github.io/jq/) for more elaborate processing.
```sh
$ structs get woh7iu3tieB0.numbers.two | jq -r 'map(.) | @csv'
2,"2nd"
```

### Fetch an individual field
Referencing a primitive field (string, number, boolean) is usually more useful than fetching structures.
```sh
$ structs get woh7iu3tieB0.numbers.two.ordinal
"2nd"
```

By default, individual fields are formatted as JSON. Use the `-r` or `--raw` flag to print a field's value instead of its JSON representation.
```sh
$ structs get --raw woh7iu3tieB0.numbers.two.ordinal
2nd
```

### Update a data structure
We can update part of the data structure by using the `set` operation with a path to the field we are changing. (We can also replace the entire data structure by updateing the root key.)
```sh
$ structs get woh7iu3tieB0.numbers.two
{
  "cardinal": 2,
  "ordinal": "second"
}
^D
woh7iu3tieB0.numbers.two
```

### Range over keys (or indexes)
Range over and print all the keys (or indexes) in an object or array. The keys or indexes are printed in raw form, suitable for use as a component in an expression.
```sh
$ structs range woh7iu3tieB0.numbers
one
two
three

$ for key in $(structs range woh7iu3tieB0.numbers); do echo "Ordinal: $(structs get -r woh7iu3tieB0.numbers.${key}.ordinal)"; done
Ordinal: 1st
Ordinal: 2nd
Ordinal: 3rd
```
