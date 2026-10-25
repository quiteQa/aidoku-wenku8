"""Check the official CLI's subscription against the exact verified source package."""

import argparse
import hashlib
import json
from pathlib import Path
from zipfile import ZipFile


def verify(package: Path, output: Path) -> dict:
    with ZipFile(package) as archive:
        assert archive.testzip() is None, "Invalid package ZIP"
        info = json.loads(archive.read("Payload/source.json"))["info"]
        icon = archive.read("Payload/icon.png")

    index = json.loads((output / "index.min.json").read_text(encoding="utf-8"))
    assert index == json.loads((output / "index.json").read_text(encoding="utf-8"))
    assert index["name"] == "Wenku8 小说源"
    assert len(index["sources"]) == 1, "Subscription must contain exactly one source"
    item = index["sources"][0]
    for field in ("id", "name", "version", "languages"):
        assert item[field] == info[field], f"Package metadata mismatch: {field}"
    assert item["contentRating"] == info.get("contentRating", 0)
    assert item["baseURL"] == (info.get("url") or info["urls"][0])
    for field in ("altNames", "minAppVersion", "maxAppVersion"):
        assert item.get(field) == info.get(field), f"Compatibility metadata mismatch: {field}"

    stem = f"{info['id']}-v{info['version']}"
    assert item["downloadURL"] == f"sources/{stem}.aix"
    assert item["iconURL"] == f"icons/{stem}.png"
    original = package.read_bytes()
    published = (output / item["downloadURL"]).read_bytes()
    assert hashlib.sha256(published).digest() == hashlib.sha256(original).digest()
    assert (output / item["iconURL"]).read_bytes() == icon
    print(f"Verified subscription: {info['id']} v{info['version']}")
    return index


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("package", type=Path)
    parser.add_argument("output", type=Path)
    arguments = parser.parse_args()
    verify(arguments.package, arguments.output)
