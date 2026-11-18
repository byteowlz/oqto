"""Compare pinned FastCDC against real rsync on run.py's exact changed-file fixture.

Requires COPY_PROBE_HOST and an empty owned COPY_PROBE_REMOTE from mktemp on that host.
Always removes that remote directory and temporary local fixture, including on failure.
Run through `uv run --no-project`; no services or product transfer code are involved.
"""

import argparse
import hashlib
import json
import os
import platform
import re
import subprocess
import tarfile
import tempfile
import time
from pathlib import Path

import run as fixture
from rolling_reference import FILES

ROOT = Path(__file__).resolve().parent
ENGINE = ROOT / "engine/target/release/copy-action-probe"


def command(args, **kwargs):
    result = subprocess.run(args, check=False, capture_output=True, **kwargs)
    if result.returncode:
        raise RuntimeError(
            f"probe command failed: {result.stderr.decode(errors='replace')}"
        )
    return result


def cdc_chunks(path):
    start = time.perf_counter()
    result = command([str(ENGINE), "cdc", str(path)])
    chunks = [
        (int(off), int(size), digest)
        for off, size, digest in (
            line.split("\t") for line in result.stdout.decode().splitlines()
        )
    ]
    return chunks, round(time.perf_counter() - start, 6)


def measure_cdc(data):
    old_chunks = {}
    for name in FILES:
        chunks, _ = cdc_chunks(data / "v1" / name)
        with (data / "v1" / name).open("rb") as stream:
            for offset, length, digest in chunks:
                stream.seek(offset)
                old_chunks[digest] = stream.read(length)
    rows = []
    for name in FILES:
        path = data / "v2" / name
        chunks, seconds = cdc_chunks(path)
        expected = path.read_bytes()
        rebuilt = bytearray()
        literal_bytes = 0
        with path.open("rb") as stream:
            for offset, length, digest in chunks:
                if digest in old_chunks:
                    content = old_chunks[digest]
                else:
                    stream.seek(offset)
                    content = stream.read(length)
                    literal_bytes += length
                assert len(content) == length
                rebuilt.extend(content)
        sha256 = hashlib.sha256(expected).hexdigest()
        assert hashlib.sha256(rebuilt).hexdigest() == sha256
        rows.append(
            {
                "file": name,
                "literal_bytes": literal_bytes,
                "matched_bytes": len(expected) - literal_bytes,
                "chunks": len(chunks),
                "manifest_seconds": seconds,
                "sha256": sha256,
                "verified": True,
            }
        )
    return rows


def transfer_fixture(local, host, remote):
    # Only the three changed files, but their bytes come from the original whole-tree RNG order.
    with tempfile.TemporaryFile() as archive:
        with tarfile.open(fileobj=archive, mode="w") as tar:
            for version in ("v1", "v2"):
                for name in FILES:
                    tar.add(
                        local / "data" / version / name,
                        arcname=f"data/{version}/{name}",
                        recursive=False,
                    )
            tar.add(ROOT / "rolling_reference.py", arcname="rolling_reference.py")
        archive.seek(0)
        command(
            [
                "ssh",
                "-o",
                "ControlMaster=no",
                "-o",
                "ControlPath=none",
                host,
                f"tar -C {remote} -xf -",
            ],
            stdin=archive,
        )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "rolling-results.json")
    args = parser.parse_args()
    if args.output.exists():
        parser.error(
            "output already exists; choose a new --output to preserve evidence"
        )
    host, remote = os.environ["COPY_PROBE_HOST"], os.environ["COPY_PROBE_REMOTE"]
    if re.fullmatch(r"/tmp/oqto-copy-probe\.[A-Za-z0-9]+", remote) is None:
        parser.error(
            "remote must be an owned mktemp directory under /tmp/oqto-copy-probe.*"
        )
    cleanup_allowed = False
    try:
        # Refuse a populated directory: the caller explicitly owns this fresh scratch.
        command(["ssh", host, f'test -d {remote} && test -z "$(ls -A {remote})"'])
        cleanup_allowed = True
        with tempfile.TemporaryDirectory(prefix=".rolling-", dir=ROOT) as temporary:
            local = Path(temporary)
            fixture.ROOT = local
            fixture.generate()
            cdc = measure_cdc(local / "data")
            transfer_fixture(local, host, remote)
            rolling = json.loads(
                command(
                    [
                        "ssh",
                        host,
                        f"cd {remote} && LC_ALL=C uv run --no-project rolling_reference.py",
                    ]
                ).stdout
            )
            expected_hashes = {row["file"]: row["sha256"] for row in cdc}
            assert all(
                row["sha256"] == expected_hashes[row["file"]] for row in rolling["rows"]
            )
            evidence = {
                "fixture": "run.py generate(), Random(8291), unchanged 2000 small files omitted from comparison",
                "parsync_commit": "998f3b412186a0c4ad4e9656e361b4c0b6d97221",
                "cdc_profile": "fastcdc=5.0.0 v2020 defaults, min=65536 avg=262144 max=1048576; BLAKE3=1.8.7",
                "cdc_platform": f"{platform.system()} {platform.machine()}",
                "rolling_placement": "local basis/source; no SSH in measured rsync runs",
                "rustc_version": command(["rustc", "--version"])
                .stdout.decode()
                .strip(),
                "probe_sources_sha256": {
                    name: hashlib.sha256((ROOT / name).read_bytes()).hexdigest()
                    for name in (
                        "rolling_compare.py",
                        "rolling_reference.py",
                        "run.py",
                        "engine/Cargo.toml",
                        "engine/Cargo.lock",
                        "engine/src/main.rs",
                    )
                },
                "rolling_flags": [
                    "-a",
                    "--no-whole-file",
                    "--checksum-choice=xxh128",
                    "--stats",
                ],
                "limitations": "Different hosts: times are NOT comparable. Literal bytes exclude signatures/ops/manifests. Not parsync or paired-WSS throughput, crash/resume or product proof.",
                "cdc": cdc,
                "rolling": rolling,
            }
            args.output.write_text(json.dumps(evidence, indent=2) + "\n")
            print(
                json.dumps(
                    {
                        "cdc_literal_bytes": sum(row["literal_bytes"] for row in cdc),
                        "rolling_literal_bytes_by_block_setting": [
                            sum(
                                row["literal_bytes"]
                                for row in rolling["rows"]
                                if row["block_bytes"] == block and row["trial"] == 1
                            )
                            for block in (None, 32768, 65536, 131072)
                        ],
                    }
                )
            )
    finally:
        if cleanup_allowed:
            command(["ssh", host, f"rm -rf -- {remote}; test ! -e {remote}"])


if __name__ == "__main__":
    main()
