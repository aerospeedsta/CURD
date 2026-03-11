import os
import sys
import time
import subprocess
import json
import random

def run_curd(cmd_args, cwd="."):
    curd_bin = os.path.abspath("target/release/curd")
    if not os.path.exists(curd_bin):
        # Try debug if release not found
        curd_bin = os.path.abspath("target/debug/curd")
    
    if not os.path.exists(curd_bin):
        print("Error: curd binary not found in target/release or target/debug")
        sys.exit(1)

    result = subprocess.run([curd_bin] + cmd_args, cwd=cwd, capture_output=True, text=True)
    return result

def main():
    repo_path = "/Users/bharath/workshop/expts/curd-bench/linux"
    if not os.path.exists(repo_path):
        # Fallback to current directory for a smoke test if bench repo not found
        repo_path = os.getcwd()
        print(f"Warning: Bench repo not found. Using current directory: {repo_path}")

    print(f"=== CURD Search Performance Benchmark (v0.7.1) ===")
    print(f"Target: {repo_path}")

    # 1. Initialize CURD
    print("Initializing CURD index (forced scope)...")
    start = time.time()
    # We use 'doctor' to ensure full index build
    run_curd(["doctor", "--index-scope", "forced"], cwd=repo_path)
    print(f"Initialization/Doctor took {time.time() - start:.2f}s")

    # 2. Fetch some symbols for testing
    print("Fetching symbols for random lookups...")
    # Use 'search sys' as a broad query to get some real symbols
    res = run_curd(["search", "sys"], cwd=repo_path)
    
    symbols = []
    try:
        data = json.loads(res.stdout)
        for item in data:
            name = item["id"].split("::")[-1]
            if "(" in name:
                name = name.split("(")[0].split()[-1]
            if len(name) > 3:
                symbols.append(name)
    except Exception as e:
        print(f"Warning: Failed to parse JSON search results: {e}")
    
    if not symbols:
        print("Warning: Could not fetch enough symbols from search results. Output was:")
        print(res.stdout[:500])
        print("Using defaults.")
        symbols = ["sys_clone", "tcp_v4_rcv", "kmalloc", "inode", "device_add", "vfs_read", "do_fork"]
    
    test_queries = random.sample(symbols, min(len(symbols), 30))
    # Add multi-word and partial queries as per plan
    test_queries.extend(["vfs read", "tcp receive", "alloc skb", "clone thread", "sys_cl", "kmall"])

    latencies = []
    
    print(f"Performing {len(test_queries)} search iterations...")
    for query in test_queries:
        t0 = time.perf_counter()
        run_curd(["search", query, "--limit", "10"], cwd=repo_path)
        t1 = time.perf_counter()
        latencies.append((t1 - t0) * 1000) # ms

    latencies.sort()
    mean_lat = sum(latencies) / len(latencies)
    p99_lat = latencies[int(len(latencies) * 0.99)] if latencies else 0
    
    print(f"\nResults:")
    print(f"  Mean Latency: {mean_lat:.2f}ms")
    print(f"  P99 Latency:  {p99_lat:.2f}ms")
    print(f"  Iterations:   {len(test_queries)}")
    print(f"  Min/Max:      {latencies[0]:.2f}ms / {latencies[-1]:.2f}ms")

if __name__ == "__main__":
    main()
