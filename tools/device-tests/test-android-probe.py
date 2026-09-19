#!/usr/bin/env python3
"""Tooling tests: no connected device, installation or real caller data."""
import argparse
import hashlib
import importlib.util
import io
import json
from pathlib import Path
import re
import struct
import sys
import tempfile
import unittest
from unittest.mock import patch
from zipfile import ZipFile

sys.dont_write_bytecode = True
spec = importlib.util.spec_from_file_location("android_probe", Path(__file__).with_name("android-probe.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)
SEED = str(json.loads((probe.ROOT / "tools/entry-limit-harness/harness-defaults.json").read_text())["syntheticBase"])


def elf(alignment=16384):
    data = bytearray(120)
    data[:6] = b"\x7fELF\x02\x01"
    struct.pack_into("<Q", data, 32, 64)
    struct.pack_into("<HH", data, 54, 56, 1)
    struct.pack_into("<I", data, 64, 1)
    struct.pack_into("<Q", data, 112, alignment)
    return data


class ProbeToolTests(unittest.TestCase):
    def test_private_input_and_no_embedded_personal_defaults(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            number = root / "number.txt"
            number.write_text("+" + SEED + "\n")
            fixture = probe.fixture_bytes("exact", "host-test", number)
            with patch.object(probe, "GENERATED", root / "generated"):
                probe.prepare(fixture)
                self.assertEqual((root / "generated/probe.properties").read_bytes(), fixture)
                self.assertEqual((root / "generated").stat().st_mode & 0o777, 0o700)
                self.assertEqual((root / "generated/probe.properties").stat().st_mode & 0o777, 0o600)
            self.assertNotIn(b"number=", probe.fixture_bytes("empty", "control", None))
            for mode, alias, value in [("exact", "test", None), ("empty", "test", number), ("empty", "../bad", None), ("other", "test", None)]:
                with self.assertRaises(ValueError):
                    probe.fixture_bytes(mode, alias, value)
            for text in [SEED, "+" + SEED + "x", "+" + SEED + "\nmode=empty", " +" + SEED]:
                number.write_text(text)
                with self.assertRaises(ValueError) as error:
                    probe.fixture_bytes("exact", "test", number)
                self.assertNotIn(text, str(error.exception))

    def test_elf_load_alignment_not_just_zip_alignment(self):
        probe.verify_elf_16kb(elf())
        for bad in [b"", bytes(120), elf(4096), elf(16385), elf()[:100]]:
            with self.assertRaises(ValueError):
                probe.verify_elf_16kb(bad)

    def test_apk_inventory_fixture_permissions_and_signature_checks(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "build-tools/36.0.0").mkdir(parents=True)
            fixture = probe.fixture_bytes("empty", "control", None)
            apk = root / "probe.apk"
            with ZipFile(apk, "w") as archive:
                archive.writestr("assets/probe.properties", fixture)
                archive.writestr("classes.dex", b"Lcom/antoniopantano/callerfilter/ScreeningService;")
                for abi in probe.ABIS:
                    for library in ("libcallerfilter_core.so", "libjnidispatch.so"):
                        archive.writestr(f"lib/{abi}/{library}", elf())
            manifest = ".ScreeningService android.permission.BIND_SCREENING_SERVICE android.telecom.CallScreeningService"
            with patch.object(probe.subprocess, "check_output", side_effect=[manifest, ""]) as output, patch.object(probe.subprocess, "run") as run:
                probe.verify_apk(apk, fixture, root, root, io.StringIO())
                self.assertEqual(len(run.call_args_list), 2)
                self.assertIn("apksigner", run.call_args_list[0].args[0][0])
                self.assertIn("zipalign", run.call_args_list[1].args[0][0])
                self.assertEqual(output.call_count, 2)
            with self.assertRaises(ValueError):
                probe.verify_apk(apk, b"wrong fixture", root, root, io.StringIO())
            with patch.object(probe.subprocess, "check_output", side_effect=[manifest, "android.permission.READ_CONTACTS"]), patch.object(probe.subprocess, "run"):
                with self.assertRaises(ValueError):
                    probe.verify_apk(apk, fixture, root, root, io.StringIO())
            with ZipFile(apk, "a") as archive:
                archive.writestr("lib/x86/libjnidispatch.so", elf())
            with self.assertRaises(ValueError):
                probe.verify_apk(apk, fixture, root, root, io.StringIO())

    def test_install_requires_selected_device_and_unchanged_recorded_apk(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            apk = root / "probe.apk"
            apk.write_bytes(b"test artifact")
            (root / "build-record.json").write_text(json.dumps({"sha256": hashlib.sha256(apk.read_bytes()).hexdigest()}))
            args = argparse.Namespace(command="install", apk=str(apk), serial="chosen-test-device")
            with patch.object(probe, "OUTPUT", root), patch.object(probe, "sdk_environment", return_value=(root, {})), patch.object(probe.subprocess, "run") as run, patch("builtins.print"):
                probe.device_command(args)
                self.assertEqual(run.call_args.args[0], [str(root / "platform-tools/adb"), "-s", args.serial, "install", "-r", str(apk)])
                apk.write_bytes(b"changed")
                with self.assertRaises(ValueError):
                    probe.device_command(args)
                args.serial = ""
                with self.assertRaises(ValueError):
                    probe.device_command(args)
                self.assertEqual(run.call_count, 1)  # no launch, role request, uninstall or call
            args.serial = "chosen-test-device"
            with patch.object(probe, "OUTPUT", root / "other"), patch.object(probe, "sdk_environment", return_value=(root, {})):
                with self.assertRaises(ValueError):
                    probe.device_command(args)

    def test_static_role_and_diagnostic_contract(self):
        # Source-level regression guard only, not an Android RoleManager/logcat test.
        sources = probe.ROOT / "apps/android/app/src/main/java/com/antoniopantano/callerfilter"
        service = (sources / "ScreeningService.kt").read_text()
        store = (sources / "ProbeStore.kt").read_text()
        self.assertIn("probeRole(this) != ProbeRole.HELD", service)
        self.assertIn("ProbeDecision(ProbeReason.ROLE_UNAVAILABLE)", service)
        for branch in (
            "!manager.isRoleAvailable(RoleManager.ROLE_CALL_SCREENING) -> ProbeRole.UNAVAILABLE",
            "manager.isRoleHeld(RoleManager.ROLE_CALL_SCREENING) -> ProbeRole.HELD",
            "else -> ProbeRole.NOT_GRANTED",
            "catch (_: RuntimeException) { ProbeRole.ERROR }",
        ):
            self.assertIn(branch, store)
        records = re.findall(r'ProbeDiagnostics\.record\("[^"]+", mapOf\((.*?)\)\)', service, re.S)
        self.assertEqual(len(records), 4)
        for fields in records:
            self.assertNotRegex(fields, r'schemeSpecificPart|phoneNumber|\$(?:callDetails|details)|(?:callDetails|details)\.toString')
            self.assertNotRegex(fields, r'"(?:number|caller|raw_handle)"\s+to')
        self.assertIn('"handle" to if (callDetails.handle?.scheme == "tel") "tel" else "unsupported"', service)
        self.assertIn('"framework_acceptance" to "unverified"', service)

    def test_log_capture_is_private_does_not_clear_or_overwrite(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory).resolve()
            destination = root / "target/device-tests/capture.log"
            args = argparse.Namespace(command="logs", serial="chosen-test-device", output=str(destination))
            with patch.object(probe, "ROOT", root), patch.object(probe, "sdk_environment", return_value=(root, {})), patch.object(probe.subprocess, "run") as run:
                probe.device_command(args)
                self.assertEqual(destination.stat().st_mode & 0o777, 0o600)
                command = run.call_args.args[0]
                self.assertIn("-s", command)
                self.assertNotIn("-c", command)
                with self.assertRaises(FileExistsError):
                    probe.device_command(args)
                args.output = str(root / "outside.log")
                with self.assertRaises(ValueError):
                    probe.device_command(args)
                self.assertEqual(run.call_count, 1)


if __name__ == "__main__":
    unittest.main()
