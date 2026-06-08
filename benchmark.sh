#!/bin/bash

if ! command -v hyperfine &> /dev/null; then
    echo "Error: hyperfine is not installed. Install it first."
    exit 1
fi

if [ ! -f /usr/bin/time ]; then
    echo "Error: GNU time is not installed at /usr/bin/time. Install it first."
    exit 1
fi

echo "=== Building pure release binary ==="
cargo build --release

echo -e "\n=== 1. Execution Time (hyperfine) ==="
hyperfine --warmup 2 './target/release/cxt --no-sort -p ./mock_repo'

echo -e "\n=== 2. Peak RAM & CPU Usage (OS Time) ==="
if [[ "$OSTYPE" == "darwin"* ]]; then
    /usr/bin/time -l ./target/release/cxt --no-sort -p ./mock_repo > /dev/null
else
    /usr/bin/time -v ./target/release/cxt --no-sort -p ./mock_repo > /dev/null
fi

echo -e "\n=== 3. Internal Heap Profiling (dhat) ==="
cargo build --release --features dhat-heap
./target/release/cxt  --no-sort -p ./mock_repo > /dev/null
echo "Heap profile saved to dhat-heap.json. View it at: https://nnethercote.github.io/dh_view/dh_view.html"
