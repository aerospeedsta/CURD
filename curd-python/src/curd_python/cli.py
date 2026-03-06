import os
import sys
import subprocess
import shutil

def main():
    """
    CURD CLI Wrapper for PyPI distributions.
    In a full production release, this routes arguments to the native
    Rust CLI binary distributed alongside the Python wheel.
    """
    args = sys.argv[1:]
    
    # Attempt to locate the native binary
    local_binary = os.path.abspath(os.path.join(os.path.dirname(__file__), "../../target/release/curd"))
    
    if os.path.exists(local_binary):
        command = local_binary
    else:
        command = shutil.which("curd")
        if not command:
            print("\033[91mError: Native `curd` binary not found in PATH.\033[0m", file=sys.stderr)
            print("Please ensure CURD is installed correctly or built via `make release`.", file=sys.stderr)
            sys.exit(1)

    try:
        result = subprocess.run([command] + args)
        sys.exit(result.returncode)
    except KeyboardInterrupt:
        sys.exit(130)

if __name__ == "__main__":
    main()