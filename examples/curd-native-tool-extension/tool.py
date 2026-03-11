#!/usr/bin/env python3

import json
import sys


def main() -> int:
    request = json.load(sys.stdin)
    raw = str(request.get("input", ""))
    response = {
        "ok": True,
        "tool": "curd_demo_tool",
        "input": raw,
        "length": len(raw),
        "normalized": raw.strip().lower(),
    }
    json.dump(response, sys.stdout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
