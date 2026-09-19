#!/usr/bin/env python3
"""Prepare/build only by default. Install/log capture require an explicit device."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import struct
import subprocess
import sys
import tempfile
from zipfile import ZipFile

ROOT = Path(__file__).resolve().parents[2]
OUTPUT = ROOT / "target/device-tests/android"
GENERATED = ROOT / "apps/android/app/generated/probeAssets"
PACKAGE = "com.antoniopantano.callerfilter"
ABIS = ("arm64-v8a", "armeabi-v7a", "x86_64")


def fixture_bytes(mode, alias, number_file):
    if not re.fullmatch(r"[A-Za-z0-9_-]+", alias):
        raise ValueError("Use a non-personal letters/digits/hyphen/underscore alias.")
    result = f"version=1\nalias={alias}\nmode={mode}\n"
    if mode == "exact":
        if number_file is None:
            raise ValueError("Exact mode requires --number-file with a prevalidated international number.")
        number = Path(number_file).read_text().rstrip("\r\n")
        if not re.fullmatch(r"\+[1-9][0-9]*", number):
            raise ValueError("Expected one canonical international number; input was not printed.")
        result += "number=" + number + "\n"
    elif mode != "empty" or number_file is not None:
        raise ValueError("Empty mode accepts no number; other modes are unsupported.")
    return result.encode()


def sdk_environment():
    env = os.environ.copy()
    sdk = Path(env.get("ANDROID_HOME") or env.get("ANDROID_SDK_ROOT") or "/opt/homebrew/share/android-commandlinetools")
    if not sdk.is_dir():
        raise ValueError("Set ANDROID_HOME to an installed SDK; this script never installs SDK components.")
    env["ANDROID_HOME"] = str(sdk)
    return sdk, env


def prepare(fixture):
    GENERATED.mkdir(parents=True, exist_ok=True)
    GENERATED.chmod(0o700)
    asset = GENERATED / "probe.properties"
    asset.write_bytes(fixture)
    asset.chmod(0o600)


def verify_elf_16kb(data):
    """Check LOAD segments, not just ZIP offsets, for our two 64-bit Android ABIs."""
    if len(data) < 64 or data[:6] != b"\x7fELF\x02\x01":
        raise ValueError("Expected a little-endian ELF64 library.")
    offset = struct.unpack_from("<Q", data, 32)[0]
    size, count = struct.unpack_from("<HH", data, 54)
    if size < 56 or count == 0 or offset + size * count > len(data):
        raise ValueError("Invalid ELF program headers.")
    loads = 0
    for index in range(count):
        header = offset + size * index
        if struct.unpack_from("<I", data, header)[0] == 1:
            loads += 1
            alignment = struct.unpack_from("<Q", data, header + 48)[0]
            if alignment < 16384 or alignment & (alignment - 1):
                raise ValueError("Native LOAD segment is not aligned for 16 KB pages.")
    if not loads:
        raise ValueError("Missing ELF LOAD segments.")


def verify_apk(apk, fixture, sdk, run_dir, log):
    def run(command):
        subprocess.run(command, stdout=log, stderr=subprocess.STDOUT, check=True)
    versions = sorted((path for path in (sdk / "build-tools").iterdir()
                       if re.fullmatch(r"\d+\.\d+\.\d+", path.name)),
                      key=lambda path: tuple(int(x) for x in path.name.split(".")))
    if not versions:
        raise ValueError("No stable Android build-tools installed.")
    tools = versions[-1]
    with ZipFile(apk) as archive:
        if archive.read("assets/probe.properties") != fixture:
            raise ValueError("Packaged fixture mismatch.")
        native_abis = {name.split("/")[1] for name in archive.namelist()
                       if name.startswith("lib/") and name.endswith(".so")}
        if native_abis != set(ABIS):
            raise ValueError("APK advertises an ABI without the expected Rust/JNA pair.")
        for abi in ABIS:
            for library in ("libcallerfilter_core.so", "libjnidispatch.so"):
                name = f"lib/{abi}/{library}"
                if archive.getinfo(name).file_size == 0:
                    raise ValueError("Empty native library.")
                if abi in ("arm64-v8a", "x86_64"):
                    verify_elf_16kb(archive.read(name))
        if not any(b"Lcom/antoniopantano/callerfilter/ScreeningService;" in archive.read(name)
                   for name in archive.namelist() if name.endswith(".dex")):
            raise ValueError("Screening service missing from DEX.")
    run([str(tools / "apksigner"), "verify", str(apk)])
    run([str(tools / "zipalign"), "-c", "-P", "16", "4", str(apk)])
    manifest = subprocess.check_output([str(tools / "aapt2"), "dump", "xmltree", str(apk), "--file", "AndroidManifest.xml"], text=True)
    for required in (".ScreeningService", "android.permission.BIND_SCREENING_SERVICE", "android.telecom.CallScreeningService"):
        if required not in manifest:
            raise ValueError("Required screening declaration missing.")
    permissions = subprocess.check_output([str(tools / "aapt2"), "dump", "permissions", str(apk)], text=True)
    for permission in ("READ_CONTACTS", "READ_CALL_LOG", "WRITE_CALL_LOG", "READ_SMS", "SEND_SMS", "POST_NOTIFICATIONS", "ANSWER_PHONE_CALLS"):
        if "android.permission." + permission in permissions:
            raise ValueError("Unexpected phone/contact/notification permission.")
    (run_dir / "manifest.txt").write_text(manifest)
    (run_dir / "permissions.txt").write_text(permissions)


def build(args):
    fixture = fixture_bytes(args.mode, args.alias, args.number_file)
    sdk, env = sdk_environment()
    OUTPUT.mkdir(parents=True, exist_ok=True)
    run_dir = Path(tempfile.mkdtemp(prefix=args.alias + "-", dir=OUTPUT))
    run_dir.chmod(0o700)
    (run_dir / "probe.properties").write_bytes(fixture)
    log_path = run_dir / "build.log"
    commands = [
        ["bash", "tools/build-android.sh"],
        [sys.executable, "-B", "tools/device-tests/test-android-probe.py"],
        ["gradle", "-p", "tools/device-tests/android-host-tests", "hostTest", "--offline", "--console=plain"],
        ["gradle", "-p", "apps/android", ":app:assembleDebug", ":app:lintDebug", "--console=plain",
         "-Pandroid.builder.sdkDownload=false"] + ([] if args.online else ["--offline"]),
    ]
    with log_path.open("w") as log:
        try:
            subprocess.run(commands[0], cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
            # Native generation replaces generated/; write the private asset afterwards.
            prepare(fixture)
            for command in commands[1:]:
                subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
            apk = run_dir / "caller-filter-probe.apk"
            shutil.copy2(ROOT / "apps/android/app/build/outputs/apk/debug/app-debug.apk", apk)
            verify_apk(apk, fixture, sdk, run_dir, log)
        except (subprocess.CalledProcessError, ValueError, KeyError) as error:
            log.write(f"\nProbe gate failed: {error}\n")
            raise ValueError(f"Build/verification failed. Private log: {log_path}") from None
    (run_dir / "build-record.json").write_text(json.dumps({
        "revision": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "dirty": bool(subprocess.check_output(["git", "status", "--porcelain"], cwd=ROOT)),
        "alias": args.alias, "mode": args.mode, "commands": commands,
        "apk": str(apk), "sha256": hashlib.sha256(apk.read_bytes()).hexdigest(),
        "abis": ABIS, "signing": "development debug key", "physical_silence_verified": False,
    }, indent=2) + "\n")
    print("PASS: fresh native bindings, host tests, APK, lint, packaging and signature.")
    print("APK:", apk)
    print("Private log:", log_path)
    print("No installation, role request or calls performed. On-device native startup is unverified.")


def device_command(args):
    sdk, _ = sdk_environment()
    if not args.serial.strip():
        raise ValueError("An explicit device serial is required.")
    adb = str(sdk / "platform-tools/adb")
    if args.command == "install":
        apk = Path(args.apk).resolve()
        if not apk.is_relative_to(OUTPUT.resolve()):
            raise ValueError("Install a verified probe APK under target/device-tests/android/.")
        record = json.loads((apk.parent / "build-record.json").read_text())
        if record["sha256"] != hashlib.sha256(apk.read_bytes()).hexdigest():
            raise ValueError("APK differs from its verified build record.")
        subprocess.run([adb, "-s", args.serial, "install", "-r", str(apk)], check=True)
        print("Installed only. Open the probe and explicitly grant the screening role.")
    else:
        destination = Path(args.output).resolve()
        if not destination.is_relative_to((ROOT / "target/device-tests").resolve()):
            raise ValueError("Capture private logs under target/device-tests/.")
        destination.parent.mkdir(parents=True, exist_ok=True)
        with destination.open("x") as log:
            destination.chmod(0o600)
            subprocess.run([adb, "-s", args.serial, "logcat", "-T", "1", "-v", "epoch", "CallerFilterProbe:I", "AndroidRuntime:E", "*:S"], stdout=log, check=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("prepare", "build"):
        command = commands.add_parser(name)
        command.add_argument("--mode", choices=("empty", "exact"), required=True)
        command.add_argument("--alias", required=True)
        command.add_argument("--number-file", type=Path)
        if name == "build":
            command.add_argument("--online", action="store_true", help="Allow Gradle dependency downloads, not SDK installation")
    install = commands.add_parser("install")
    install.add_argument("--serial", required=True)
    install.add_argument("--apk", required=True)
    logs = commands.add_parser("logs")
    logs.add_argument("--serial", required=True)
    logs.add_argument("--output", required=True)
    args = parser.parse_args()
    try:
        if args.command == "prepare":
            prepare(fixture_bytes(args.mode, args.alias, args.number_file))
            print("Private generated fixture prepared only. No APK build or device action.")
        elif args.command == "build":
            build(args)
        else:
            device_command(args)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        print(str(error) if isinstance(error, ValueError) else "Tool/file operation failed.", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
