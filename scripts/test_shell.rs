fn main() {
    let status = std::process::Command::new("sandbox-exec")
        .arg("-p").arg("(version 1)\n(deny default)\n(allow process-exec)\n(allow process-fork)\n")
        .arg("sh").arg("-c").arg("echo hello")
        .status().unwrap();
    println!("Code: {:?}", status.code());
}
