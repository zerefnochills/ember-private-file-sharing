#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
python_command="${PYTHON:-python3}"
if ! command -v "$python_command" >/dev/null 2>&1; then
  python_command=python
fi

"$python_command" - "$repo_root/android/app/src/main/res/values/strings.xml" <<'PY'
import re
import sys
import xml.etree.ElementTree as ET

path = sys.argv[1]
root = ET.parse(path).getroot()

strings = {}
for node in root.findall("string"):
    name = node.attrib.get("name")
    if not name:
        raise SystemExit("Found string tag without name attribute")
    if name in strings:
        raise SystemExit(f"Duplicate string key found: {name}")
    text = "".join(node.itertext())
    strings[name] = text

# Check format arguments
pattern = re.compile(r"%(?:[1-9][0-9]*\$)?[0-9]*[a-zA-Z%]")
for key, text in strings.items():
    if "%" in text:
        matches = pattern.findall(text)
        # Check for unescaped dangling percent signs
        dangling = re.sub(r"%(?:[1-9][0-9]*\$)?[0-9]*[a-zA-Z%]", "", text)
        if "%" in dangling:
            raise SystemExit(f"Malformed or unescaped % found in {key}: {text}")

print(f"English resource keys and format arguments validated ({len(strings)} strings).")
PY
