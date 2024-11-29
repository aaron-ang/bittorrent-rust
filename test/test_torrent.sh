#!/bin/bash

current_dir=$(dirname "$(readlink -f "$0")")
parent_dir=$(dirname "$current_dir")
test_script="$parent_dir/your_bittorrent.sh"
torrents=$(find "$current_dir" -name '*.torrent')
output_files=()

trap cleanup INT

cleanup() {
    echo "Removing output files..."
    for file in "${output_files[@]}"; do
        rm "$file"
    done
    exit 1
}

for torrent in $torrents; do
    echo "Testing $torrent"
    output_file="${torrent%.torrent}"
    $test_script download -o "$output_file" "$torrent"
    if [[ $? -ne 0 ]]; then
        echo "Failed to download $torrent"
        break
    fi
    output_files+=("$output_file")
done

cleanup
