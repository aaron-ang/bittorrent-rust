#!/bin/bash

current_dir=$(dirname "$0")
parent_dir=$(dirname "$current_dir")
test_script="$parent_dir/your_bittorrent.sh"
torrents=$(find "$current_dir" -name '*.torrent')
magnet_file="$current_dir/magnet_links.txt"
output_files=()

for torrent in $torrents; do
    echo "Testing $torrent"
    output_file="${torrent%.torrent}"
    $test_script download -o "$output_file" "$torrent"
    output_files+=("$output_file")
done

while read magnet; do
    output_file="$current_dir"/$(echo "$magnet" | awk -F ':' '{print $1}' | sed 's/.torrent//')
    magnet_url=$(echo "$magnet" | awk -F ': ' '{print $2}')
    echo "Testing $magnet_url"
    $test_script magnet_download -o "$output_file" "$magnet_url"
    output_files+=("$output_file")
done <"$magnet_file"

for file in "${output_files[@]}"; do
    rm "$file"
done
