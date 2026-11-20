#!/usr/bin/env python3
"""Discover or qualify/update exact latest-stable Pi source pins; never deploy."""

import argparse
import base64
import contextlib
import fcntl
import hashlib
import io
import json
import os
import re
import subprocess
import sys
import tarfile
import tempfile
import tomllib
import urllib.parse
import urllib.request
from pathlib import Path

PACKAGE = "@earendil-works/pi-coding-agent"
REGISTRY = "https://registry.npmjs.org/@earendil-works%2fpi-coding-agent"
RELEASES = "https://api.github.com/repos/earendil-works/pi/releases/tags/"
STABLE = re.compile(r"(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\Z")
MAX_METADATA = 16 * 1024 * 1024
PIN_PATHS = (
    "dependencies.toml",
    "dist/manifest.toml",
    "scripts/dist/pi-node/package.json",
    "scripts/dist/pi-node/package-lock.json",
)


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
    validate_integrity(integrity)
    expected = f"https://registry.npmjs.org/{PACKAGE}/-/pi-coding-agent-{version}.tgz"
    if metadata.get("dist", {}).get("tarball") != expected:
        raise ValueError("unexpected official SDK tarball origin/path")
    return integrity


def validate_integrity(integrity):
    if not integrity.startswith("sha512-"):
        raise ValueError("official npm closure requires SHA512 integrity")
    if len(base64.b64decode(integrity[7:], validate=True)) != 64:
        raise ValueError("invalid SHA512 digest length")


def discover(current, metadata):
    current_key = version_key(current)
    if not isinstance(metadata, dict):
        raise TypeError("publisher metadata must be an object")
    latest = metadata.get("dist-tags", {}).get("latest")
    latest_key = version_key(latest)
    sdk = metadata.get("versions", {}).get(latest)
    if not isinstance(sdk, dict):
        raise TypeError("stable channel has no matching package metadata")
    integrity = validate_sdk(sdk, latest)
    if latest_key < current_key:
        raise ValueError("refusing upstream downgrade; source pins unchanged")
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
    prefix = f"https://github.com/earendil-works/pi/releases/download/v{version}/"
    assets = {
        a.get("name"): a.get("browser_download_url") for a in release.get("assets", [])
    }
    names = ["pi-linux-x64.tar.gz", "pi-linux-arm64.tar.gz"]
    for name in ["SHA256SUMS", *names]:
        if assets.get(name) != prefix + name:
            raise ValueError(f"official release asset missing/mismatched: {name}")
    sums = {}
    for line in text.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[1].lstrip("*") in names:
            name = parts[1].lstrip("*")
            if name in sums or not re.fullmatch("[0-9a-f]{64}", parts[0]):
                raise ValueError("duplicate/malformed publisher checksum")
            sums[name] = parts[0]
    if len(sums) != 2:
        raise ValueError("publisher Linux checksum manifest is incomplete")
    return sums


def update_manifest(text, values, section="external"):
    start = text.index(f"[{section}]")
    end = text.find("\n[", start + len(section) + 2)
    end = len(text) if end < 0 else end
    body = text[start:end]
    for key, value in values.items():
        body, count = re.subn(
            r"(?m)^(" + re.escape(key) + r'\s*=\s*")[^"]*(".*)$',
            lambda match, value=value: match[1] + value + match[2],
            body,
        )
        if count != 1:
            raise ValueError(f"non-unique/missing {section} pin: {key}")
    return text[:start] + body + text[end:]


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
            validate_integrity(entry.get("integrity", ""))


def publisher_asset(release, version, name):
    matches = [a for a in release["assets"] if a.get("name") == name]
    if len(matches) != 1:
        raise ValueError(f"non-unique/missing release asset: {name}")
    asset = matches[0]
    url = f"https://github.com/earendil-works/pi/releases/download/v{version}/{name}"
    if asset.get("browser_download_url") != url:
        raise ValueError("release asset origin mismatch")
    data = fetch(url)
    if asset.get("digest") != "sha256:" + hashlib.sha256(data).hexdigest():
        raise ValueError(f"release asset digest mismatch: {name}")
    return data


def hydrate_workspaces(lock):
    for path, entry in lock["packages"].items():
        if not path or (entry.get("resolved") and entry.get("integrity")):
            continue
        prefix = "node_modules/@earendil-works/"
        if not path.startswith(prefix) or "/" in path[len(prefix) :]:
            raise ValueError(f"unverified non-workspace lock entry: {path}")
        name = path.removeprefix("node_modules/")
        version = entry["version"]
        version_key(version)
        url = (
            "https://registry.npmjs.org/"
            + urllib.parse.quote(name, safe="")
            + "/"
            + version
        )
        metadata = json.loads(fetch(url))
        if metadata.get("name") != name or metadata.get("version") != version:
            raise ValueError("workspace registry identity mismatch")
        expected = (
            f"https://registry.npmjs.org/{name}/-/{name.split('/')[-1]}-{version}.tgz"
        )
        dist = metadata["dist"]
        if dist.get("tarball") != expected:
            raise ValueError("workspace registry origin mismatch")
        validate_integrity(dist["integrity"])
        entry.update(resolved=expected, integrity=dist["integrity"])


def candidate_closure(source, plan):
    version = plan["candidate"]
    release = json.loads(fetch(RELEASES + "v" + version))
    sums = release_sums(
        release, version, publisher_asset(release, version, "SHA256SUMS").decode()
    )
    package = json.loads(
        publisher_asset(release, version, "pi-coding-agent-install-package.json")
    )
    lock = json.loads(
        publisher_asset(release, version, "pi-coding-agent-install-package-lock.json")
    )
    if package.get("version") != version or lock.get("lockfileVersion") != 3:
        raise ValueError("publisher installer version/lock format mismatch")
    existing = json.loads((source / PIN_PATHS[2]).read_text())
    package.update(name=existing["name"], version=existing["version"], private=True)
    lock.update(name=package["name"], version=package["version"])
    lock["packages"][""].update(name=package["name"], version=package["version"])
    hydrate_workspaces(lock)
    validate_lock({"external": {"pi": version}}, package, lock, plan["npm_integrity"])
    return package, lock, sums


def render_pins(source, plan, package, lock, sums):
    pins = tomllib.loads((source / PIN_PATHS[0]).read_text())["external"]
    engine = plan.get("node_engine")
    if not isinstance(engine, str) or not re.fullmatch(r">=\d+\.\d+\.\d+", engine):
        raise ValueError(
            "unsupported Node engine contract; reviewed Node update needed"
        )
    if version_key(pins["node"]) < version_key(engine[2:]):
        raise ValueError("latest Pi needs a separately checksum-qualified newer Node")
    encoded_lock = json.dumps(lock, indent=2) + "\n"
    values = {
        "pi": plan["candidate"],
        **{k.removesuffix(".tar.gz") + "-sha256": v for k, v in sums.items()},
    }
    dependencies = update_manifest((source / PIN_PATHS[0]).read_text(), values)
    manifest = update_manifest(
        (source / PIN_PATHS[1]).read_text(),
        {
            "pi_version": plan["candidate"],
            "pi_npm_integrity": plan["npm_integrity"],
            "pi_package_lock_sha256": hashlib.sha256(encoded_lock.encode()).hexdigest(),
        },
        section="release",
    )
    return dict(
        zip(
            PIN_PATHS,
            [
                dependencies,
                manifest,
                json.dumps(package, indent=2) + "\n",
                encoded_lock,
            ],
        )
    )


def clean_source(source):
    result = subprocess.check_output(
        ["git", "-C", str(source), "status", "--porcelain"], text=True
    )
    if result:
        raise ValueError(
            "daily update requires a clean canonical checkout; no stash/reset/second tree"
        )


@contextlib.contextmanager
def source_lock(source):
    name = subprocess.check_output(
        ["git", "-C", str(source), "rev-parse", "--git-path", "pi-update.lock"],
        text=True,
    ).strip()
    with (source / name).open("a") as handle:
        fcntl.flock(handle, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield


def shipped_extensions(source, evidence):
    revision = tomllib.loads((source / PIN_PATHS[0]).read_text())["external"][
        "pi-extensions"
    ]
    if not re.fullmatch("[0-9a-f]{40}", revision):
        raise ValueError("extensions require an exact public commit")
    names = [
        line.strip()
        for line in (source / "scripts/dist/pi-default-extensions.txt")
        .read_text()
        .splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    ]
    if not names or any(not re.fullmatch("pi-[a-z0-9-]+", n) for n in names):
        raise ValueError("invalid shipped extension names")
    data = fetch(
        f"https://api.github.com/repos/byteowlz/pi-agent-extensions/tarball/{revision}"
    )
    destination = evidence / "extension-input"
    destination.mkdir()
    with tarfile.open(fileobj=io.BytesIO(data)) as archive:
        members = [
            m
            for m in archive.getmembers()
            if len(Path(m.name).parts) > 1 and Path(m.name).parts[1] in names
        ]
        archive.extractall(destination, members=members, filter="data")
    roots = list(destination.iterdir())
    if len(roots) != 1 or any(not (roots[0] / n / "index.ts").is_file() for n in names):
        raise ValueError("pinned public archive lacks shipped extensions")
    return roots[0]


def qualify(source, evidence):
    runtime = evidence / "runtime"
    home = evidence / "home"
    home.mkdir()
    env = {
        "PATH": str(Path(sys.executable).parent) + os.pathsep + os.defpath,
        "HOME": str(home),
        "LANG": "C.UTF-8",
    }
    if os.environ.get("PI_NODE_SOURCE_ARCHIVE"):
        env["PI_NODE_SOURCE_ARCHIVE"] = os.environ["PI_NODE_SOURCE_ARCHIVE"]
    version = tomllib.loads((source / PIN_PATHS[0]).read_text())["external"]["pi"]
    extensions = shipped_extensions(source, evidence)
    commands = [
        ["bash", str(source / "scripts/dist/stage-pi-node-runtime.sh"), str(runtime)],
        [
            sys.executable,
            str(source / "scripts/dist/test-pi-rpc-qualification.py"),
            "--binary",
            str(runtime / "pi"),
            "--expected-version",
            version,
            "--output",
            str(evidence / "rpc"),
        ],
        [
            str(runtime / "node/bin/node"),
            str(source / "scripts/dist/test-shipped-extensions.mjs"),
            str(runtime / "pi"),
            str(extensions),
            version,
        ],
        [sys.executable, str(source / "scripts/tests/test-pi-node-runtime.py")],
        ["bash", str(source / "scripts/tests/test-stage-pi-runtime-idempotent.sh")],
    ]
    for number, command in enumerate(commands):
        with (evidence / f"gate-{number}.log").open("wb") as output:
            subprocess.run(
                command,
                cwd=source,
                env=env,
                stdout=output,
                stderr=subprocess.STDOUT,
                check=True,
                timeout=600,
            )


def prepare_changes(source, evidence, plan, verify):
    for name in PIN_PATHS:
        path = source / name
        if path.is_symlink() or not path.resolve().is_relative_to(source):
            raise ValueError(f"source pin is not a contained regular file: {name}")
    evidence.mkdir(parents=True, mode=0o700, exist_ok=False)
    originals = {name: (source / name).read_bytes() for name in PIN_PATHS}
    package, lock, sums = candidate_closure(source, plan)
    changes = render_pins(source, plan, package, lock, sums)
    if verify and originals != {n: c.encode() for n, c in changes.items()}:
        raise ValueError("rebased pins differ from the publisher closure; rerun update")
    return originals, changes


def write_pin(path, data):
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(
            dir=path.parent, prefix=".pi-pin-", delete=False
        ) as output:
            temporary = Path(output.name)
            os.fchmod(output.fileno(), path.stat().st_mode & 0o777)
            output.write(data)
            output.flush()
            os.fsync(output.fileno())
        os.replace(temporary, path)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


@contextlib.contextmanager
def applied_pins(source, originals, changes):
    success = False
    try:
        for name, content in changes.items():
            write_pin(source / name, content.encode())
        yield
        if any((source / n).read_bytes() != c.encode() for n, c in changes.items()):
            raise ValueError("source pins changed concurrently during qualification")
        success = True
    finally:
        if not success:
            for name, data in originals.items():
                if (source / name).read_bytes() == changes[name].encode():
                    write_pin(source / name, data)


@contextlib.contextmanager
def failed_report_cleanup(evidence):
    try:
        yield
    except BaseException:
        (evidence / "report.json").unlink(missing_ok=True)
        raise


def update_source(source, evidence, metadata, verify=False):
    source = source.resolve(strict=True)
    evidence = evidence.resolve()
    with source_lock(source):
        clean_source(source)
        pins = tomllib.loads((source / PIN_PATHS[0]).read_text())
        plan = discover(pins["external"]["pi"], metadata)
        if plan["major_review_required"]:
            raise ValueError(
                "latest stable is a new major: reviewed integration required, not silently skipped"
            )
        if plan["status"] == "current" and not verify:
            return plan
        if verify and plan["status"] != "current":
            raise ValueError("latest stable moved; rerun the update, not verification")
        originals, changes = prepare_changes(source, evidence, plan, verify)
        with failed_report_cleanup(evidence), applied_pins(source, originals, changes):
            qualify(source, evidence)
            result = {
                **plan,
                "status": "verified" if verify else "updated",
                "qualified": True,
                "qualification_scope": "source-runtime-rpc-and-extension-smoke-not-deployment",
                "accepted_pin_changed": originals
                != {n: (source / n).read_bytes() for n in PIN_PATHS},
                "lock_sha256": hashlib.sha256(
                    (source / PIN_PATHS[3]).read_bytes()
                ).hexdigest(),
            }
            (evidence / "report.json").write_text(json.dumps(result, indent=2) + "\n")
        return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--source", type=Path, default=Path(__file__).resolve().parents[2]
    )
    parser.add_argument(
        "--metadata-json", type=Path, help="offline publisher metadata fixture"
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--update", action="store_true", help="qualify and update four source-pin files"
    )
    mode.add_argument(
        "--verify",
        action="store_true",
        help="requalify latest source pins after a clean rebase",
    )
    parser.add_argument(
        "--evidence",
        type=Path,
        help="NEW evidence/build directory; must be outside the checkout",
    )
    args = parser.parse_args()
    source = args.source.resolve(strict=True)
    metadata = json.loads(
        args.metadata_json.read_bytes() if args.metadata_json else fetch(REGISTRY)
    )
    if args.update or args.verify:
        if not args.evidence or args.evidence.resolve().is_relative_to(source):
            parser.error(
                "--update/--verify requires NEW --evidence outside the checkout"
            )
        result = update_source(source, args.evidence.resolve(), metadata, args.verify)
    else:
        current = tomllib.loads((source / PIN_PATHS[0]).read_text())["external"]["pi"]
        result = discover(current, metadata)
    print(json.dumps(result, indent=2))


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
