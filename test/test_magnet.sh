#!/bin/bash

current_dir=$(dirname "$(readlink -f "$0")")
parent_dir=$(dirname "$current_dir")
test_script="$parent_dir/your_bittorrent.sh"
magnet_file="$current_dir/magnet_links.txt"
output_files=()

trap cleanup INT

cleanup() {
    echo "Removing output files..."
    for file in "${output_files[@]}"; do
        rm "$file"
    done
    exit 1
}

while read magnet; do
    output_file="$current_dir"/$(echo "$magnet" | awk -F ':' '{print $1}' | sed 's/.torrent//')
    magnet_url=$(echo "$magnet" | awk -F ': ' '{print $2}')
    echo "Testing $magnet_url"
    $test_script magnet_download -o "$output_file" "$magnet_url"
    if [[ $? -ne 0 ]]; then
        echo "Failed to download $magnet_url"
        break
    fi
    output_files+=("$output_file")
done <"$magnet_file"

cleanup
