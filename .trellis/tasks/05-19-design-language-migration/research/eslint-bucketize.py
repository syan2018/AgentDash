"""Aggregate ESLint warnings per feature module.

Run: cd packages/app-web && pnpm exec eslint src --no-fix --format json | python <this>
"""
import json
import sys
import re
import collections

data = json.load(sys.stdin)
buckets = collections.Counter()
files_per_bucket = collections.defaultdict(list)

for entry in data:
    fp = entry["filePath"].replace("\\", "/")
    m = re.search(r"src/(features/[^/]+|pages|components/[^/]+|components|stores|hooks|lib|utils)", fp)
    bucket = m.group(1) if m else "other"
    if entry["warningCount"]:
        buckets[bucket] += entry["warningCount"]
        files_per_bucket[bucket].append((entry["warningCount"], fp.split("src/")[-1]))

print(f"{'count':>5}  bucket")
for k, v in buckets.most_common():
    print(f"{v:5d}  {k}")
print("-" * 40)
print(f"{sum(buckets.values()):5d}  TOTAL")
print()
print("=== top files per bucket ===")
for bucket, _ in buckets.most_common():
    print(f"\n[{bucket}]")
    for cnt, path in sorted(files_per_bucket[bucket], reverse=True):
        print(f"  {cnt:3d}  {path}")
