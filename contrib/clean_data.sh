#!/bin/bash

# Threshold in bytes (21 MiB)
TH=21
THRESHOLD=$(($TH * 1024 * 1024))

# Search paths for large directories
set -- "/tmp/floresta-func-tests/data" "/tmp/floresta-func-tests/logs" $(find . -type d -name 'tmp-db')

for search in "$@"; do
    if [ -d "$search" ]; then
        while IFS= read -r -d '' d; do
            size=$(du -sb "$d" 2>/dev/null | cut -f1)
            if [ -n "$size" ] && [ "$size" -gt "$THRESHOLD" ]; then
                dirs+=("$d")
            fi
        done < <(find "$search" -mindepth 1 -maxdepth 1 -type d -print0)
    fi
done

# Check if there is anything to delete
if [ ${#dirs[@]} -eq 0 ]; then
    echo "No directories larger than $TH MiB found."
    exit 0
fi

# Display the directories that will be deleted
echo "The following directories exceed $TH MiB and will be deleted:"
for d in "${dirs[@]}"; do
    size_human=$(du -sh "$d" 2>/dev/null | cut -f1)
    echo "  $d ($size_human)"
done

# Prompt the user for confirmation
read -r -p "Are you sure you want to delete all those directories listed above? [y/N] " ans

# Check the user's response
if [[ "$ans" =~ ^[Yy]$ ]]; then
    for d in "${dirs[@]}"; do
        rm -rf "$d"
    done

    echo "Directories deleted."
else
    echo "Deletion cancelled."
fi
