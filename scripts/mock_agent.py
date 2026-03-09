import sys
import json
import subprocess
import time

def run_test(bin_path, workspace, request):
    # Start the curd process
    proc = subprocess.Popen(
        [bin_path, workspace],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True
    )
    
    try:
        # Send the request and close stdin immediately
        stdout, stderr = proc.communicate(input=request + "\n", timeout=60)
        # return the first line of stdout
        lines = [l for l in stdout.split('\n') if l.strip()]
        if not lines:
            return f"ERROR_EMPTY_STDOUT: {stderr}"
        return lines[0]
    except subprocess.TimeoutExpired:
        proc.kill()
        stdout, stderr = proc.communicate()
        return f"TIMEOUT: stderr={stderr}"
    except Exception as e:
        return f"ERROR: {str(e)}"

if __name__ == "__main__":
    if len(sys.argv) < 4:
        print("Usage: mock_agent.py <bin> <ws> <request_json>")
        sys.exit(1)
    
    print(run_test(sys.argv[1], sys.argv[2], sys.argv[3]))
