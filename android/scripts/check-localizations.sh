#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
python_command="${PYTHON:-python3}"
if ! command -v "$python_command" >/dev/null 2>&1; then
  python_command=python
fi

"$python_command" - "$repo_root/android/app/src/main/res/values/strings.xml" \
  "$repo_root/android/app/src/main/res/values-hi/strings.xml" <<'PY'
import re
import sys
import xml.etree.ElementTree as ET

def load(path):
    root = ET.parse(path).getroot()
    return {
        node.attrib["name"]: "".join(node.itertext())
        for node in root.findall("string")
    }

english = load(sys.argv[1])
hindi = load(sys.argv[2])
missing_hi = sorted(english.keys() - hindi.keys())
missing_en = sorted(hindi.keys() - english.keys())
if missing_hi or missing_en:
    raise SystemExit(f"Localization keys differ: missing hi={missing_hi}; missing en={missing_en}")

pattern = re.compile(r"%(?:[1-9][0-9]*\$)?[0-9]*[a-zA-Z%]")
for key in sorted(english):
    en_args = sorted(pattern.findall(english[key]))
    hi_args = sorted(pattern.findall(hindi[key]))
    if en_args != hi_args:
        raise SystemExit(f"Format arguments differ for {key}: en={en_args}; hi={hi_args}")
print(f"English/Hindi resource keys and format arguments match ({len(english)} strings).")
PY
