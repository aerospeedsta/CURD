#!/bin/bash
# CURD Enterprise Benchmark Script - Release Mode
# Measures Rapid Initial Indexing and "fzf-like" Search Performance

REPOS=("rust" "pytorch" "kubernetes" "linux" "godot")
CURD_BIN="/Users/bharath/workshop/expts/CURD/target/release/curd"
RESULTS_FILE="/Users/bharath/workshop/expts/CURD/curd_bench_results.md"

echo "# CURD Performance Benchmark" > $RESULTS_FILE
echo "| Repository | Files | Symbols | Initial Index (s) | Fast Search (ms) |" >> $RESULTS_FILE
echo "| :--- | :--- | :--- | :--- | :--- |" >> $RESULTS_FILE

for REPO in "${REPOS[@]}"; do
    REPO_PATH="/Users/bharath/workshop/expts/$REPO"
    if [ ! -d "$REPO_PATH" ]; then
        echo "Skipping $REPO: directory not found at $REPO_PATH"
        continue
    fi
    echo "Benchmarking $REPO..."
    
    cd $REPO_PATH
    # Ensure a fresh state for initial indexing benchmark
    rm -rf .curd/curd_state.sqlite3
    
    $CURD_BIN init > /dev/null
    
    # 1. Initial Indexing
    START=$(date +%s%N)
    # Using --index-mode full to force a complete parse for the benchmark
    $CURD_BIN doctor . --index-mode full > /dev/null
    END=$(date +%s%N)
    INDEX_TIME_SEC=$(echo "scale=3; ($END - $START) / 1000000000" | bc)
    
    # Extract stats from database
    FILE_COUNT=$(sqlite3 .curd/curd_state.sqlite3 "SELECT COUNT(*) FROM files;")
    SYMBOL_COUNT=$(sqlite3 .curd/curd_state.sqlite3 "SELECT COUNT(*) FROM symbols;")
    
    # 2. Fast Search (Cache Hit Path)
    START_MS=$(date +%s%N)
    $CURD_BIN search "main" > /dev/null
    END_MS=$(date +%s%N)
    SEARCH_TIME_MS=$(echo "($END_MS - $START_MS) / 1000000" | bc)
    
    echo "| $REPO | $FILE_COUNT | $SYMBOL_COUNT | $INDEX_TIME_SEC | $SEARCH_TIME_MS |" >> $RESULTS_FILE
done

echo "Benchmark complete. Results saved to $RESULTS_FILE"
cat $RESULTS_FILE
