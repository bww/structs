## Structs, the data structure service
_Structs_ endeavors to provide a missing capability in shell scripting: working with structured data from external sources. Structs allows you to parse a JSON data structure and maintain a data structure in memory so you can access fields directly in a natural way.

```json
{
    "page": {...},
    "data": [
        {
            "size": "S",
            "name": "Small"
        },
        {
            "size": "M",
            "name": "Medium"
        },
        {
            "size": "L",
            "name": "Large"
        },
        ...
    ]
}
```

Given this API response, we can use Structs to process our data: 

```sh
key=$(curl -sSL 'https://api.website.com/widgets/sizes' | structs set)
for i in $(structs range ${key}.data); do
  echo "Size $(structs get -r ${key}.data.${i}.size): $(structs get -r ${key}.data.${i}.name)"
done
```

Which will yield something like:

```
Size S: Small
Size M: Medium
Size L: Large
...
```

Recent versions of Bash and Zsh support indexed and associative arrays, however neither excels at the common use case of interacting with structured data from an external source. Most likely, this pattern will involve repeatedly processing and parsing JSON strings to extract each field of interest and use it. Far from ideal.

