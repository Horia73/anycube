#!/usr/bin/env python3
"""Pack the native macOS lane's signed anycube.app as an updater asset."""

import argparse
import gzip
import json
from pathlib import Path
import re
import stat
import tarfile
import tempfile

LIMIT = 192 * 1024 * 1024
EXECUTABLES = {"Contents/MacOS/anycube", "Contents/MacOS/anycube-server"}
MANIFEST = "Contents/Resources/desktop-update.json"


def package(bundle: Path, output_dir: Path) -> Path:
    if bundle.name != "anycube.app" or bundle.is_symlink() or not bundle.is_dir():
        raise ValueError("Expected a real anycube.app directory")
    if output_dir.resolve().is_relative_to(bundle.resolve()):
        raise ValueError("Write update assets outside the app bundle")
    paths = [bundle, *sorted(bundle.rglob("*"))]
    if len(paths) > 2048:
        raise ValueError("Too many bundle entries")
    seen = set()
    total = 10_240
    for path in paths:
        name = path.relative_to(bundle.parent).as_posix()
        if name not in {"anycube.app", "anycube.app/Contents"} and not name.startswith("anycube.app/Contents/"):
            raise ValueError(f"Unexpected bundle path {name}")
        if any(not re.fullmatch(r"[A-Za-z0-9._ -]+", part) or part.endswith((".", " "))
               for part in name.split("/")) or name.lower() in seen:
            raise ValueError(f"Invalid or duplicate bundle path {name}")
        seen.add(name.lower())
        attrs = path.lstat()
        if not (stat.S_ISREG(attrs.st_mode) or stat.S_ISDIR(attrs.st_mode)):
            raise ValueError(f"Unsupported bundle entry {name}")
        total += 512
        if stat.S_ISREG(attrs.st_mode):
            total += (attrs.st_size + 511) // 512 * 512
            if bool(attrs.st_mode & 0o111) != (path.relative_to(bundle).as_posix() in EXECUTABLES):
                raise ValueError(f"Unexpected executable permission on {name}")
    if total > LIMIT:
        raise ValueError("Expanded desktop archive exceeds the size limit")
    for name in EXECUTABLES | {MANIFEST, "Contents/Info.plist"}:
        if not (bundle / name).is_file():
            raise ValueError(f"Missing {name}")
    manifest_path = bundle / MANIFEST
    if manifest_path.stat().st_size > 4096:
        raise ValueError("Desktop update manifest exceeds the size limit")
    metadata = json.loads(manifest_path.read_bytes())
    arch, channel, build, version = (metadata[key] for key in ("architecture", "channel", "build", "version"))
    if (set(metadata) != {"architecture", "channel", "build", "version", "platform"}
        or metadata["platform"] != "macos" or arch not in {"arm64", "x86_64"}
        or not re.fullmatch(r"[a-z0-9-]{1,40}", channel)
        or type(build) is not int or not 0 < build < 2**64
        or not re.fullmatch(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", version)):
        raise ValueError("Invalid desktop update manifest")
    output_dir.mkdir(parents=True, exist_ok=True)
    output = output_dir / f"anycube-desktop-macos-{arch}-{channel}-build-{build}.tar.gz"
    with tempfile.TemporaryDirectory(dir=output_dir) as scratch:
        temporary = Path(scratch) / output.name
        with temporary.open("wb") as dest:
            with gzip.GzipFile(filename="", fileobj=dest, mode="wb", mtime=0) as compressed:
                with tarfile.open(fileobj=compressed, mode="w", format=tarfile.USTAR_FORMAT, dereference=True) as archive:
                    for path in paths:
                        name = path.relative_to(bundle.parent).as_posix()
                        archive.add(path, arcname=name, recursive=False)
        if temporary.stat().st_size > LIMIT:
            raise ValueError("Compressed desktop archive exceeds the size limit")
        temporary.replace(output)
    return output


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("bundle", type=Path)
    parser.add_argument("output_dir", type=Path)
    args = parser.parse_args()
    print(package(args.bundle, args.output_dir))
