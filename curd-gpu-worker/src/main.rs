use anyhow::Result;
use curd_core::gpu::ComputeBackend;
use pollster::block_on;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

#[derive(Deserialize)]
struct Request {
    strings: Vec<String>,
}

#[derive(Serialize)]
struct Response {
    hashes: Vec<String>,
}

fn main() -> Result<()> {
    let backend = match ComputeBackend::new() {
        Ok(Some(b)) => b,
        _ => {
            eprintln!("No GPU backend available in worker");
            std::process::exit(1);
        }
    };

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    for line_result in stdin.lock().lines() {
        let line = line_result?;
        let req: Request = match serde_json::from_str(&line) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Worker failed to parse request: {}", e);
                continue;
            }
        };
        let str_refs: Vec<&str> = req.strings.iter().map(|s| s.as_str()).collect();
        let hashes = block_on(backend.hash_batch(&str_refs))?;
        let res = Response { hashes };
        let res_json = serde_json::to_string(&res)?;
        writeln!(stdout, "{}", res_json)?;
        stdout.flush()?;
    }

    Ok(())
}
