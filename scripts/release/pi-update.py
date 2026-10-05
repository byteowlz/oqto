#!/usr/bin/env python3
"""Discover official stable Pi and prepare a NEW isolated exact-pin candidate.

Preparation is not qualification, pin promotion, installation or publication.
No user config, source worktree or accepted release pin is ever overwritten.
"""

import argparse
import base64
import hashlib
import io
import json
import re
import subprocess
import sys
import tarfile
import urllib.request
from pathlib import Path

import tomllib

PACKAGE = "@earendil-works/pi-coding-agent"
REGISTRY = "https://registry.npmjs.org/@earendil-works%2fpi-coding-agent"
RELEASES = "https://api.github.com/repos/earendil-works/pi/releases/tags/"
STABLE = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\Z")
MAX_METADATA = 16 * 1024 * 1024


def version_key(value):
    if not isinstance(value, str) or not STABLE.fullmatch(value):
        raise ValueError(f"not an exact stable Pi version: {value!r}")
    return tuple(map(int, value.split(".")))


def fetch(url):
    request = urllib.request.Request(
        url,
        headers={"User-Agent": "oqto-pi-qualification", "Accept": "application/json"},
    )
    with urllib.request.urlopen(request, timeout=30) as response:
        data = response.read(MAX_METADATA + 1)
    if len(data) > MAX_METADATA:
        raise ValueError("publisher metadata exceeds bounded size")
    return data


def validate_sdk(metadata, version):
    if metadata.get("name") != PACKAGE or metadata.get("version") != version:
        raise ValueError("publisher package name/version mismatch")
    integrity = metadata.get("dist", {}).get("integrity", "")
    if not integrity.startswith("sha512-"):
        raise ValueError("official SDK requires SHA512 integrity")
    digest = base64.b64decode(integrity[7:], validate=True)
    if len(digest) != 64:
        raise ValueError("invalid SHA512 digest length")
    expected = f"https://registry.npmjs.org/{PACKAGE}/-/pi-coding-agent-{version}.tgz"
    if metadata.get("dist", {}).get("tarball") != expected:
        raise ValueError("unexpected official SDK tarball origin/path")
    return integrity


def discover(current, metadata):
    current_key = version_key(current)
    # Use the publisher stable channel, never silently pick a prerelease or
    # an abandoned higher version from the complete historical registry list.
    if not isinstance(metadata, dict):
        raise TypeError("publisher metadata must be an object")
    latest = metadata.get("dist-tags", {}).get("latest")
    latest_key = version_key(latest)
    sdk = metadata.get("versions", {}).get(latest)
    if not isinstance(sdk, dict):
        raise TypeError("stable channel has no matching package metadata")
    integrity = validate_sdk(sdk, latest)
    if latest_key < current_key:
        raise ValueError("refusing upstream downgrade; accepted pins unchanged")
    return {
        "schema": 1,
        "package": PACKAGE,
        "current": current,
        "candidate": latest,
        "status": "current" if latest == current else "candidate",
        "major_review_required": latest_key[0] != current_key[0],
        "npm_integrity": integrity,
        "tarball": sdk["dist"]["tarball"],
        "node_engine": sdk.get("engines", {}).get("node"),
        "qualified": False,
        "accepted_pin_changed": False,
    }


def release_sums(release, version, text):
    if (
        release.get("tag_name") != "v" + version
        or release.get("draft")
        or release.get("prerelease")
    ):
        raise ValueError("GitHub release is not the exact official stable release")
    expected_prefix = (
        f"https://github.com/earendil-works/pi/releases/download/v{version}/"
    )
    assets = {
        item.get("name"): item.get("browser_download_url")
        for item in release.get("assets", [])
    }
    for name in ["SHA256SUMS", "pi-linux-x64.tar.gz", "pi-linux-arm64.tar.gz"]:
        if assets.get(name) != expected_prefix + name:
            raise ValueError(f"official release asset missing/mismatched: {name}")
    sums = {}
    for line in text.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[1].lstrip("*") in [
            "pi-linux-x64.tar.gz",
            "pi-linux-arm64.tar.gz",
        ]:
            name = parts[1].lstrip("*")
            if name in sums or not re.fullmatch("[0-9a-f]{64}", parts[0]):
                raise ValueError("duplicate/malformed publisher checksum")
            sums[name] = parts[0]
    if len(sums) != 2:
        raise ValueError("publisher Linux checksum manifest is incomplete")
    return sums


def update_manifest(text, values):
    start = text.index("[external]")
    end = text.find("\n[", start + len("[external]"))
    end = len(text) if end < 0 else end
    section = text[start:end]
    for key, value in values.items():
        section, count = re.subn(
            r"(?m)^(" + re.escape(key) + r'\s*=\s*")[^"]*(".*)$',
            lambda match, value=value: match[1] + value + match[2],
            section,
        )
        if count != 1:
            raise ValueError(f"non-unique/missing external pin: {key}")
    return text[:start] + section + text[end:]


def validate_lock(manifest, package, lock, sdk_integrity):
    version = manifest["external"]["pi"]
    version_key(version)
    if package.get("dependencies") != {PACKAGE: version}:
        raise ValueError("runtime manifest must contain the exact single Pi dependency")
    packages = lock.get("packages", {})
    sdk = packages.get("node_modules/" + PACKAGE, {})
    if (
        packages.get("", {}).get("dependencies") != package["dependencies"]
        or sdk.get("version") != version
        or sdk.get("integrity") != sdk_integrity
    ):
        raise ValueError("runtime/package/lock/publisher integrity disagreement")
    for name, entry in packages.items():
        if name:
            if not entry.get("resolved", "").startswith("https://registry.npmjs.org/"):
                raise ValueError(f"non-registry closure entry: {name}")
            integrity = entry.get("integrity", "")
            if (
                not integrity.startswith("sha512-")
                or len(base64.b64decode(integrity[7:], validate=True)) != 64
            ):
                raise ValueError(f"unverified dependency integrity: {name}")


def prepare(source, output, plan, node, release, sums):
    if plan["status"] != "candidate" or plan["major_review_required"]:
        raise ValueError(
            "no forward same-major candidate; major versions need reviewed admission"
        )
    version_key(plan["candidate"])
    if output.exists() or output.is_symlink():
        raise ValueError(
            "candidate output must be NEW; existing paths are never replaced"
        )
    # git archive is an immutable tracked snapshot: no dirty files, user homes,
    # hooks, credentials, Git config or shared working-tree mutations are copied.
    base = subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "HEAD"], text=True
    ).strip()
    if not re.fullmatch("[0-9a-f]{40}", base):
        raise ValueError("candidate base is not an immutable commit")
    archived_manifest = subprocess.check_output(
        ["git", "-C", str(source), "show", base + ":dependencies.toml"], text=True
    )
    original = tomllib.loads(archived_manifest)
    if original["external"]["pi"] != plan["current"]:
        raise ValueError("accepted pin changed since discovery; re-discover")
    # Only the already checksum-qualified Node runtime is used for lock creation.
    engine = plan.get("node_engine")
    if not isinstance(engine, str) or not re.fullmatch(
        r">=(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)", engine
    ):
        raise ValueError(
            "unsupported Node engine contract; requires reviewed qualification"
        )
    if version_key(original["external"]["node"]) < version_key(engine[2:]):
        raise ValueError("candidate requires a separately qualified newer Node runtime")
    node = node.resolve(strict=True)
    receipt = json.loads((node.parents[2] / "oqto-runtime.json").read_text())
    if (
        receipt.get("node_version") != original["external"]["node"]
        or receipt.get("node_archive_sha256")
        != original["external"]["node-linux-x64-sha256"]
    ):
        raise ValueError(
            "Node runtime receipt does not match accepted Node qualification"
        )
    version = subprocess.check_output([str(node), "--version"], text=True).strip()
    if version != "v" + original["external"]["node"]:
        raise ValueError("Node binary version disagrees with qualification receipt")
    output.mkdir(parents=False, mode=0o700)
    tree = output / "source"
    tree.mkdir(mode=0o700)
    tar = subprocess.check_output(
        ["git", "-C", str(source), "archive", "--format=tar", base]
    )
    with tarfile.open(fileobj=io.BytesIO(tar)) as archive:
        archive.extractall(tree, filter="data")
    values = {
        "pi": plan["candidate"],
        "pi-linux-x64-sha256": sums["pi-linux-x64.tar.gz"],
        "pi-linux-arm64-sha256": sums["pi-linux-arm64.tar.gz"],
    }
    (tree / "dependencies.toml").write_text(update_manifest(archived_manifest, values))
    package_path = tree / "scripts/dist/pi-node/package.json"
    package = json.loads(package_path.read_text())
    package["dependencies"] = {PACKAGE: plan["candidate"]}
    package_path.write_text(json.dumps(package, indent=2) + "\n")
    home = output / "private-home"
    home.mkdir(mode=0o700)
    for name in ["npmrc", "globalnpmrc"]:
        (home / name).write_text("")
    env = {
        "PATH": str(node.parent) + ":/usr/bin:/bin",
        "HOME": str(home),
        "NPM_CONFIG_CACHE": str(output / "npm-cache"),
        "NPM_CONFIG_UPDATE_NOTIFIER": "false",
    }
    npm = node.parent.parent / "lib/node_modules/npm/bin/npm-cli.js"
    with (output / "lock-generation.log").open("wb") as log:
        subprocess.run(
            [
                str(node),
                str(npm),
                "install",
                "--package-lock-only",
                "--ignore-scripts",
                "--engine-strict",
                "--no-audit",
                "--no-fund",
                "--userconfig",
                str(home / "npmrc"),
                "--globalconfig",
                str(home / "globalnpmrc"),
            ],
            cwd=package_path.parent,
            env=env,
            stdout=log,
            stderr=subprocess.STDOUT,
            check=True,
            timeout=300,
        )
    lock = json.loads((package_path.parent / "package-lock.json").read_text())
    validate_lock(
        tomllib.loads((tree / "dependencies.toml").read_text()),
        package,
        lock,
        plan["npm_integrity"],
    )
    result = {
        **plan,
        "status": "prepared",
        "base_commit": base,
        "source": str(tree),
        "lock_sha256": hashlib.sha256(
            (package_path.parent / "package-lock.json").read_bytes()
        ).hexdigest(),
        "archive_checksums": sums,
        "qualification_required": True,
        "release_metadata": release,
    }
    (output / "candidate.json").write_text(json.dumps(result, indent=2) + "\n")
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source", type=Path, default=Path(__file__).resolve().parents[2]
    )
    parser.add_argument(
        "--metadata-json",
        type=Path,
        help="offline publisher fixture; never invokes mutation alone",
    )
    parser.add_argument("--prepare", type=Path, metavar="NEW_DIRECTORY")
    parser.add_argument(
        "--node",
        type=Path,
        help="qualified release-owned node/bin/node; required for preparation",
    )
    args = parser.parse_args()
    current = tomllib.loads((args.source / "dependencies.toml").read_text())[
        "external"
    ]["pi"]
    metadata = json.loads(
        args.metadata_json.read_bytes() if args.metadata_json else fetch(REGISTRY)
    )
    plan = discover(current, metadata)
    if args.prepare:
        if not args.node:
            parser.error("--prepare requires --node")
        release = json.loads(fetch(RELEASES + "v" + plan["candidate"]))
        sum_url = f"https://github.com/earendil-works/pi/releases/download/v{plan['candidate']}/SHA256SUMS"
        sums = release_sums(release, plan["candidate"], fetch(sum_url).decode())
        plan = prepare(args.source, args.prepare, plan, args.node, release, sums)
    print(json.dumps(plan, indent=2))


if __name__ == "__main__":
    try:
        main()
    except (
        ValueError,
        TypeError,
        KeyError,
        OSError,
        subprocess.SubprocessError,
    ) as error:
        print(
            json.dumps(
                {
                    "schema": 1,
                    "status": "blocked",
                    "error": str(error),
                    "accepted_pin_changed": False,
                }
            ),
            file=sys.stderr,
        )
        raise SystemExit(1)
