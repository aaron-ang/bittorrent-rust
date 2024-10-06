#!/bin/bash

current_dir=$(dirname "$0")
torrents=$(find "$current_dir" -name '*.torrent')
parent_dir=$(dirname "$current_dir")
test_script="$parent_dir/your_bittorrent.sh"
output_files=$(find "$current_dir" -name '*.out')

for torrent in $torrents; do
    echo "Testing $torrent"
    output_file="${torrent%.torrent}.out"
    ./your_bittorrent.sh download -o "$output_file" "$torrent"
done

for file in $output_files; do
    rm "$file"
done
