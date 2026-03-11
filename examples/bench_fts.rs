use anyhow::Result;
use curd_core::{CurdConfig, SearchEngine, Symbol, storage::Storage};
use std::path::PathBuf;
use std::time::Instant;

fn main() -> Result<()> {
    let root = PathBuf::from("/Users/bharath/workshop/expts/curd-bench/linux");
    if !root.exists() {
        println!("Linux repo not found in bench dir. Skipping.");
        return Ok(());
    }

    let cfg = CurdConfig::default();
    let se = SearchEngine::new(&root).with_config(cfg.clone());
    let storage = Storage::open(&root, &cfg)?;

    println!("=== CURD v0.7.1-beta Search Benchmark (Linux Kernel) ===");
    
    let queries = vec!["sys_clone", "tcp_v4_rcv", "kmalloc", "inode", "device_add"];

    for query in queries {
        println!("\nQuery: '{}'", query);

        // 1. Benchmark BM25 (Ranked)
        let start = Instant::now();
        let ranked = storage.search_ranked(query, None)?;
        let ranked_dur = start.elapsed();
        let ranked_count = ranked.len();
        println!("  BM25 Ranked:   {:>8?} | Results: {}", ranked_dur, ranked_count);

        // 2. Benchmark Substring (Fallback)
        let start = Instant::now();
        let mut stmt = storage.conn.prepare("SELECT id FROM symbols WHERE name LIKE ?1 LIMIT 50")?;
        let _ = stmt.query_map([format!("%{}%", query)], |r| r.get::<_, String>(0))?.count();
        let fallback_dur = start.elapsed();
        println!("  Substring:     {:>8?} | (LIKE %query%)", fallback_dur);
        
        let speedup = fallback_dur.as_secs_f64() / ranked_dur.as_secs_f64();
        println!("  Speedup:       {:.2}x", speedup);
    }

    Ok(())
}
