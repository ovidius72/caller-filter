#!/usr/bin/env python3
"""Build in private, ignored staging; install only through a separate command."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import plistlib
import re
import shutil
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[2]
HARNESS = ROOT / "tools/entry-limit-harness"
OUTPUT = ROOT / "target/device-tests/ios"
APP_ID = "com.antoniopantano.limittest"
EXT_ID = APP_ID + ".calldir"


def fixture_for(mode, alias, number_file=None, count=None):
    if not re.fullmatch(r"[A-Za-z0-9_-]+", alias):
        raise ValueError("Use a non-personal alias containing letters, digits, '-' or '_'.")
    controls = json.loads((HARNESS / "harness-defaults.json").read_text())
    fixture = {"version": 1, "alias": alias, "mode": mode,
               **{key: controls[key] for key in ("progressEvery", "retryDelaySeconds", "maximumRetries")}}
    if mode == "exact":
        if number_file is None or count is not None:
            raise ValueError("Exact mode requires --number-file and does not accept --count.")
        # Transport only. The operator supplies an already validated international number.
        fixture["number"] = Path(number_file).read_text().rstrip("\r\n")
    elif mode in ("blocking", "identification"):
        if number_file is not None or count is None or count < 0:
            raise ValueError("Capacity mode requires a nonnegative --count, without --number-file.")
        fixture.update(count=count, syntheticBase=controls["syntheticBase"])
    elif mode == "empty":
        if number_file is not None or count is not None:
            raise ValueError("Empty mode accepts neither a number nor a count.")
    else:
        raise ValueError("Unsupported mode.")
    return fixture


def stage_project(directory, fixture):
    source = directory / "source"
    source.mkdir()
    for name in ("App", "CallDir", "Shared"):
        shutil.copytree(HARNESS / name, source / name)
    shutil.copy2(HARNESS / "project.yml", source / "project.yml")
    (source / "HarnessFixture.json").write_text(json.dumps(fixture, indent=2) + "\n")
    return source


def build_command(source, directory, sign=False, device=None):
    if sign and (not device or not os.environ.get("DEVELOPMENT_TEAM")):
        raise ValueError("Signing requires --device and DEVELOPMENT_TEAM (see apps/ios/.env).")
    if device and not sign:
        raise ValueError("--device is only used with --sign; unsigned builds target generic iOS.")
    command = ["xcodebuild", "-project", str(source / "LimitTest.xcodeproj"),
               "-scheme", "LimitTest", "-configuration", "Release",
               "-destination", "id=" + device if sign else "generic/platform=iOS",
               "-derivedDataPath", str(directory / "derived"), "ENABLE_DEBUG_DYLIB=NO"]
    if not sign:
        command += ["CODE_SIGNING_ALLOWED=NO", "CODE_SIGNING_REQUIRED=NO"]
    # Never register devices or change provisioning automatically.
    return command + ["build"]


def verify_bundle(app, expected_fixture, signed=False):
    extension = app / "PlugIns/CallDir.appex"
    versions = []
    for bundle, identifier in ((app, APP_ID), (extension, EXT_ID)):
        with (bundle / "Info.plist").open("rb") as handle:
            info = plistlib.load(handle)
        if info.get("CFBundleIdentifier") != identifier:
            raise ValueError("Unexpected bundle identifier.")
        version = (info.get("CFBundleVersion"), info.get("CFBundleShortVersionString"))
        if not all(isinstance(value, str) and value for value in version):
            raise ValueError("Bundle version is missing.")
        versions.append(version)
        if not (bundle / info["CFBundleExecutable"]).is_file():
            raise ValueError("Bundle executable missing.")
        actual = json.loads((bundle / "HarnessFixture.json").read_text())
        if actual != expected_fixture:
            raise ValueError("App/extension fixture mismatch.")
        if signed and not (bundle / "embedded.mobileprovision").is_file():
            raise ValueError("Missing on-device provisioning profile.")
    if versions[0] != versions[1]:
        raise ValueError("App/extension version mismatch.")
    with (extension / "Info.plist").open("rb") as handle:
        info = plistlib.load(handle)
    config = info.get("NSExtension", {})
    if config.get("NSExtensionPointIdentifier") != "com.apple.callkit.call-directory":
        raise ValueError("Missing Call Directory extension point.")
    if not config.get("NSExtensionPrincipalClass", "").endswith(".CallDirectoryHandler"):
        raise ValueError("Missing Call Directory principal class.")


def build(args):
    fixture = fixture_for(args.mode, args.alias, args.number_file, args.count)
    # Reject invalid signing arguments before creating outputs or running any build.
    build_command(Path("source"), Path("derived"), args.sign, args.device)
    OUTPUT.mkdir(parents=True, exist_ok=True)
    directory = Path(tempfile.mkdtemp(prefix=args.alias + "-", dir=OUTPUT))
    directory.chmod(0o700)
    source = stage_project(directory, fixture)
    log_path = directory / "build.log"
    with log_path.open("w") as log:
        def run(command):
            subprocess.run(command, cwd=source, stdout=log, stderr=subprocess.STDOUT, check=True)
        validator = directory / "validate-fixture"
        try:
            run(["swiftc", str(source / "Shared/HarnessConfiguration.swift"),
                 str(ROOT / "tools/device-tests/ios-probe-config.swift"), "-o", str(validator)])
            run([str(validator), str(source / "HarnessFixture.json")])
            run(["xcodegen", "generate", "--spec", str(source / "project.yml")])
            run(build_command(source, directory, args.sign, args.device))
            app = directory / "derived/Build/Products/Release-iphoneos/LimitTest.app"
            verify_bundle(app, fixture, signed=args.sign)
            if args.sign:
                run(["codesign", "--verify", "--deep", "--strict", str(app)])
        except (subprocess.CalledProcessError, ValueError):
            raise ValueError(f"Build/validation failed. Private log: {log_path}") from None
    revision = subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip()
    dirty = bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT))
    (directory / "build-record.json").write_text(json.dumps({
        "revision": revision, "dirty": dirty, "mode": args.mode, "alias": args.alias,
        "signed": args.sign, "app": str(app),
        "sha256": {str(path.relative_to(app)): hashlib.sha256(path.read_bytes()).hexdigest()
                   for path in (app / "LimitTest", app / "HarnessFixture.json",
                                app / "PlugIns/CallDir.appex/CallDir")},
        "command": build_command(source, directory, args.sign, args.device),
        "physical_silence_verified": False,
    }, indent=2) + "\n")
    print("Build and packaged fixture checks passed.")
    print("App:", app)
    print("Private log:", log_path)
    print("Signed for selected device." if args.sign else "UNSIGNED: device build only; sign before installation.")
    print("No installation, extension reload or calls performed.")


def install(args):
    app = Path(args.app).resolve()
    if not app.is_relative_to(OUTPUT.resolve()):
        raise ValueError("Install only a probe built under target/device-tests/ios/.")
    fixture = json.loads((app / "HarnessFixture.json").read_text())
    verify_bundle(app, fixture, signed=True)
    subprocess.run(["codesign", "--verify", "--deep", "--strict", str(app)], check=True)
    subprocess.run(["xcrun", "devicectl", "device", "install", "app", "--device", args.device, str(app)], check=True)
    print("Installed. Enable the extension, open the probe and reload its bundled list manually.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    builder = commands.add_parser("build", help="Build only; unsigned unless --sign is explicit")
    builder.add_argument("--mode", choices=("empty", "exact", "blocking", "identification"), required=True)
    builder.add_argument("--alias", required=True)
    builder.add_argument("--number-file", type=Path)
    builder.add_argument("--count", type=int)
    builder.add_argument("--sign", action="store_true")
    builder.add_argument("--device")
    builder.set_defaults(action=build)
    installer = commands.add_parser("install", help="Explicit, user-authorized device installation")
    installer.add_argument("--app", required=True)
    installer.add_argument("--device", required=True)
    installer.set_defaults(action=install)
    args = parser.parse_args()
    try:
        args.action(args)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        # I/O errors may contain paths; keep output private. Never print fixture contents.
        print(str(error) if isinstance(error, ValueError) else "Tool or file operation failed.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
