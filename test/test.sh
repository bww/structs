#!/usr/bin/env bash

set -eo pipefail

# where am i?
me_home=$(dirname "$0")
me_home=$(cd "$me_home" && pwd)

key=$(structs set < "$me_home/size.json")
for i in $(structs range ${key}.data); do
  echo "Size $(structs get -r ${key}.data.${i}.size): $(structs get -r ${key}.data.${i}.name)"
done
