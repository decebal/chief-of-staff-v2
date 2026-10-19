#!/usr/bin/env python3
"""Validate and package only explicitly allowlisted starter files. No network."""
import argparse
import hashlib
import json
import re
import zipfile
from pathlib import Path, PurePosixPath

ROOT = Path(__file__).resolve().parent.parent
PRIVATE_PARTS = {".chief-of-staff", ".private", ".git", ".env", "__pycache__", "node_modules"}
PATTERNS = [
    r"(?:/Users|/home)/[^\s/]+/",
    r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----",
    r"(?:sk_live_|sk_test_|ghp_|github_pat_|xoxb-|xoxp-)[A-Za-z0-9_-]{12,}",
    r"Bearer\s+[A-Za-z0-9_.-]{24,}",
]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def checked_files():
    manifest = json.loads((ROOT / "release-files.json").read_text())
    names = manifest["files"]
    require(len(names) == len(set(names)), "Duplicate release file")
    files = {}
    for name in names:
        rel = PurePosixPath(name)
        require(not rel.is_absolute() and ".." not in rel.parts and "\\" not in name, "Unsafe release path")
        require(not PRIVATE_PARTS.intersection(rel.parts), "Private/runtime path in release")
        require(not any(part.startswith(".env.") for part in rel.parts), "Environment file in release")
        path = ROOT / name
        require(path.is_file() and not path.is_symlink(), f"Missing file or symlink: {name}")
        require(path.resolve().is_relative_to(ROOT), "File escapes package")
        # Reject symlinked parent folders as well as symlinked leaf files.
        require(not any(parent.is_symlink() for parent in path.parents if parent != ROOT and ROOT in parent.parents), "Symlinked package parent")
        blob = path.read_bytes()
        text = blob.decode("utf-8")
        for pattern in PATTERNS:
            require(re.search(pattern, text) is None, f"Possible private path or credential in {name}")
        files[name] = blob
    plugin = json.loads(files["plugins/chief-of-staff-starter/plugin.json"])
    require(plugin["name"] == "chief-of-staff-starter" and plugin["version"] == manifest["version"], "Plugin/release version mismatch")
    require(not any(key in plugin for key in ("mcpServers", "hooks", "apps")), "Unexpected integration in starter")
    onboarding = plugin["extensions"]["com.openai"]["onboardingSkill"]
    require("plugins/chief-of-staff-starter/" + onboarding.removeprefix("./") in files, "Broken onboarding reference")
    profile = json.loads(files["plugins/chief-of-staff-starter/skills/chief-of-staff-starter/assets/profile.example.json"])
    require(profile["sources"] == [] and not profile["localTracking"]["approved"], "Starter contains approved sources/tracking")
    require(not profile["schedule"]["enabled"] and not any(profile["permissions"].values()), "Starter contains active authority")
    require(not profile["recovery"]["automaticRateLimitProbe"], "Automatic retries must default off")
    marketplace = json.loads(files[".agents/plugins/marketplace.json"])
    entry = marketplace["plugins"][0]
    require(entry["source"]["path"] == "./plugins/chief-of-staff-starter" and entry["policy"]["installation"] == "AVAILABLE", "Unexpected marketplace path or auto-install policy")
    # Markdown relative links must resolve to packaged files. Skip remote links,
    # anchors and code samples (which can contain illustrative placeholders).
    for name, blob in files.items():
        if not name.endswith(".md"):
            continue
        prose = re.sub(r"```.*?```", "", blob.decode(), flags=re.S)
        for target in re.findall(r"\]\(([^)]+)\)", prose):
            if "://" in target or target.startswith("#"):
                continue
            path = (ROOT / name).parent / target.split("#")[0]
            require(path.resolve().is_relative_to(ROOT), f"Link escapes package: {name}")
            require(path.resolve().relative_to(ROOT).as_posix() in files, f"Broken packaged link in {name}: {target}")
    return manifest, files


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, help="New ZIP path; omit for checks only")
    args = parser.parse_args()
    manifest, files = checked_files()
    if args.output:
        require(not args.output.exists(), "Refusing to overwrite an existing release")
        require(not args.output.resolve().is_relative_to(ROOT), "Store release archive outside the working package")
        checksums = "".join(f"{hashlib.sha256(blob).hexdigest()}  {name}\n" for name, blob in sorted(files.items()))
        prefix = f"chief-of-staff-starter-{manifest['version']}/"
        with zipfile.ZipFile(args.output, "x", compression=zipfile.ZIP_DEFLATED) as archive:
            for name, blob in sorted(files.items()):
                info = zipfile.ZipInfo(prefix + name, date_time=(2026, 10, 16, 0, 0, 0))
                info.external_attr = 0o100644 << 16
                info.compress_type = zipfile.ZIP_DEFLATED
                archive.writestr(info, blob)
            archive.writestr(prefix + "SHA256SUMS", checksums)
        with zipfile.ZipFile(args.output) as archive:
            require(archive.testzip() is None, "Archive CRC failure")
            for name, blob in files.items():
                require(archive.read(prefix + name) == blob, "Archive readback mismatch")
        print(json.dumps({"version": manifest["version"], "files": len(files), "zip": str(args.output), "sha256": hashlib.sha256(args.output.read_bytes()).hexdigest()}))
    else:
        print(json.dumps({"version": manifest["version"], "validatedFiles": len(files), "note": "Allowlist and pattern preflight; not a substitute for privacy review"}))


if __name__ == "__main__":
    main()
