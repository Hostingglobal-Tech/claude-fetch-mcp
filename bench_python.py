"""
Python baseline: requests + BeautifulSoup + html2text
대조: Rust MCP vs 전형 Python fetch pipeline
"""
import json
import os
import statistics
import sys
import time
from pathlib import Path

try:
    import requests
except ImportError:
    print("pip install requests beautifulsoup4 html2text", file=sys.stderr)
    sys.exit(1)

from bs4 import BeautifulSoup
import html2text


def fetch_and_parse(url: str, h2t) -> tuple[int, int, int]:
    t0 = time.perf_counter()
    r = requests.get(
        url,
        timeout=30,
        headers={
            "User-Agent": "bench-python/0.1",
            "Accept-Language": "ko,en;q=0.8",
        },
        allow_redirects=True,
    )
    fetch_ms = int((time.perf_counter() - t0) * 1000)

    t1 = time.perf_counter()
    ct = r.headers.get("content-type", "")
    if "html" in ct:
        soup = BeautifulSoup(r.text, "html.parser")
        md = h2t.handle(str(soup))
    else:
        md = r.text
    parse_ms = int((time.perf_counter() - t1) * 1000)

    return fetch_ms, parse_ms, len(md)


def main():
    urls_file = sys.argv[1] if len(sys.argv) > 1 else "bench_urls.txt"
    runs = int(sys.argv[2]) if len(sys.argv) > 2 else 5
    out = sys.argv[3] if len(sys.argv) > 3 else "bench_python.json"

    urls = []
    for line in Path(urls_file).read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            urls.append(line)

    h2t = html2text.HTML2Text()
    h2t.ignore_links = False
    h2t.body_width = 0

    all_results = []
    cold_all = []
    for url in urls:
        print(f"  {url}", file=sys.stderr)
        cold = []
        md_bytes = 0
        status = 0
        err = None
        for _ in range(runs):
            try:
                f, p, b = fetch_and_parse(url, h2t)
                cold.append(f + p)
                md_bytes = b
                status = 200
            except Exception as e:
                err = str(e)
                break
        all_results.append({
            "url": url,
            "runs": runs,
            "cold_ms": cold,
            "markdown_bytes": md_bytes,
            "status": status,
            "error": err,
        })
        cold_all.extend(cold)

    report = {
        "host": os.environ.get("COMPUTERNAME", "unknown"),
        "python_version": sys.version.split()[0],
        "urls": all_results,
        "cold_mean_ms": statistics.mean(cold_all) if cold_all else 0,
        "cold_p50_ms": statistics.median(cold_all) if cold_all else 0,
        "cold_stdev_ms": statistics.stdev(cold_all) if len(cold_all) > 1 else 0,
    }
    Path(out).write_text(json.dumps(report, indent=2), encoding="utf-8")
    print(f"\npython cold mean: {report['cold_mean_ms']:.1f}ms | p50: {report['cold_p50_ms']:.1f}ms")


if __name__ == "__main__":
    main()
