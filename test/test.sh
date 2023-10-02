#!/usr/bin/env bash

set -eo pipefail

# where am i?
me_home=$(dirname "$0")
me_home=$(cd "$me_home" && pwd)

key=$(cat "$me_home/size.json" | structs --debug set)
for i in $(structs range ${key}.data); do
  echo "Size $(structs get -r ${key}.data.${i}.size): $(structs get -r ${key}.data.${i}.name)"
done
