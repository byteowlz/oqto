"""Run actual rsync's rolling matcher on synthetic local files, not a transport benchmark."""

import hashlib
import json
import platform
import re
import shutil
import subprocess
import time
from pathlib import Path

FILES = ("big/binary0.bin", "big/binary1.bin", "text.txt")
# 64 KiB is parsync's choice for our 128 MiB binaries; test its adjacent sizes too.
BLOCKS = (None, 32768, 65536, 131072)


def sha256(path):
    with path.open("rb") as stream:
        return hashlib.file_digest(stream, "sha256").hexdigest()


def parse_stats(output):
    names = {
        "literal_bytes": "Literal data",
        "matched_bytes": "Matched data",
        "sent_bytes": "Total bytes sent",
        "received_bytes": "Total bytes received",
    }
    stats = {}
    for key, label in names.items():
        match = re.search(rf"^{label}: ([\d,]+)(?: bytes)?$", output, re.MULTILINE)
        if match is None:
            raise ValueError(f"missing rsync statistic: {label}")
        stats[key] = int(match[1].replace(",", ""))
    return stats


def measure(root):
    rows = []
    for block in BLOCKS:
        for trial in range(1, 4):
            for name in FILES:
                basis, source = root / "data/v1" / name, root / "data/v2" / name
                dest = root / "reconstructed.bin"
                # Reset outside timer; local rsync otherwise defaults to whole-file.
                shutil.copyfile(basis, dest)
                args = [
                    "rsync",
                    "-a",
                    "--no-whole-file",
                    "--checksum-choice=xxh128",
                    "--stats",
                ]
                if block is not None:
                    args.append(f"--block-size={block}")
                args.extend([str(source), str(dest)])
                start = time.perf_counter()
                result = subprocess.run(
                    args, capture_output=True, text=True, check=True
                )
                seconds = round(time.perf_counter() - start, 6)
                stats = parse_stats(result.stdout)
                assert (
                    stats["literal_bytes"] + stats["matched_bytes"]
                    == source.stat().st_size
                )
                digest = sha256(source)
                assert sha256(dest) == digest
                rows.append(
                    dict(
                        file=name,
                        block_bytes=block,
                        trial=trial,
                        seconds=seconds,
                        sha256=digest,
                        verified=True,
                        **stats,
                    )
                )
    return rows


if __name__ == "__main__":
    root = Path.cwd()
    assert root.name.startswith("oqto-copy-probe.") and root.parent == Path("/tmp")
    version = subprocess.run(
        ["rsync", "--version"], capture_output=True, text=True, check=True
    )
    print(
        json.dumps(
            {
                "rsync_version": version.stdout.splitlines()[0],
                "platform": f"{platform.system()} {platform.machine()}",
                "scratch_filesystem": subprocess.run(
                    ["stat", "-f", "-c", "%T", "."],
                    capture_output=True,
                    text=True,
                    check=True,
                ).stdout.strip(),
                "rows": measure(root),
            }
        )
    )
