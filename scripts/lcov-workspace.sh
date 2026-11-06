#!/bin/sh
# Keep only workspace records in an lcov file and relativize their paths.
# Portable across GNU/BSD awk and sed (macOS legs use this too).
set -eu
in=${1:?usage: lcov-workspace.sh <in.info> <out.info>}
out=${2:?usage: lcov-workspace.sh <in.info> <out.info>}
prefix="SF:${PWD%/}/"
awk -v prefix="$prefix" '
    /^SF:/ {
        include = index($0, prefix) == 1
        if (include) {
            $0 = "SF:" substr($0, length(prefix) + 1)
            print
        }
        next
    }
    include { print }
    /^end_of_record$/ { include = 0 }
' "$in" > "$out"
