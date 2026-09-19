#!/usr/bin/env python3
import importlib.util
import json
import os
from pathlib import Path
import plistlib
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location("ios_probe", Path(__file__).with_name("ios-probe.py"))
probe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(probe)


class ProbeTests(unittest.TestCase):
    def test_fixture_modes_and_missing_inputs(self):
        self.assertEqual(probe.fixture_for("empty", "control")["mode"], "empty")
        for mode in ("blocking", "identification"):
            value = probe.fixture_for(mode, "capacity", count=4)
            self.assertEqual(value["count"], 4)
            self.assertNotIn("number", value)
        for options in (("exact", "test"), ("blocking", "test"),
                        ("empty", "../escape"), ("bad", "test")):
            with self.assertRaises(ValueError):
                probe.fixture_for(*options)
        with self.assertRaises(ValueError):
            probe.fixture_for("empty", "test", count=1)
        with self.assertRaises(ValueError):
            probe.fixture_for("blocking", "test", count=-1)

    def test_exact_number_stays_in_private_fixture(self):
        with tempfile.TemporaryDirectory() as tmp:
            number_file = Path(tmp) / "number.txt"
            number = "+" + json.loads((probe.HARNESS / "harness-defaults.json").read_text())["syntheticBase"]
            number_file.write_text(number + "\n")
            fixture = probe.fixture_for("exact", "caller", number_file=number_file)
            self.assertEqual(fixture["number"], number)
            source = probe.stage_project(Path(tmp), fixture)
            self.assertEqual(json.loads((source / "HarnessFixture.json").read_text()), fixture)
            self.assertEqual((source / "project.yml").read_bytes(), (probe.HARNESS / "project.yml").read_bytes())
            self.assertTrue((source / "Shared/HarnessLoadState.swift").exists())
            self.assertNotIn(number, (source / "App/App.swift").read_text())

    def test_signing_and_device_selection_are_explicit(self):
        command = probe.build_command(Path("source"), Path("out"))
        self.assertIn("generic/platform=iOS", command)
        self.assertIn("CODE_SIGNING_ALLOWED=NO", command)
        self.assertNotIn("-allowProvisioningUpdates", command)
        with self.assertRaises(ValueError):
            probe.build_command(Path("s"), Path("o"), device="selected-device")
        with patch.dict(os.environ, {}, clear=True):
            with self.assertRaises(ValueError):
                probe.build_command(Path("s"), Path("o"), sign=True, device="selected-device")
        with patch.dict(os.environ, {"DEVELOPMENT_TEAM": "test-team"}):
            with self.assertRaises(ValueError):
                probe.build_command(Path("s"), Path("o"), sign=True)
            command = probe.build_command(Path("s"), Path("o"), sign=True, device="selected-device")
            self.assertIn("id=selected-device", command)
            self.assertNotIn("CODE_SIGNING_ALLOWED=NO", command)
            self.assertNotIn("-allowProvisioningUpdates", command)

    def test_bundle_verification_and_signature_prerequisites(self):
        fixture = probe.fixture_for("empty", "test")
        with tempfile.TemporaryDirectory() as tmp:
            app = Path(tmp) / "LimitTest.app"
            ext = app / "PlugIns/CallDir.appex"
            for bundle, identifier in ((app, probe.APP_ID), (ext, probe.EXT_ID)):
                bundle.mkdir(parents=True)
                info = {"CFBundleIdentifier": identifier, "CFBundleExecutable": "binary",
                        "CFBundleVersion": "1", "CFBundleShortVersionString": "1.0"}
                if bundle == ext:
                    info["NSExtension"] = {
                        "NSExtensionPointIdentifier": "com.apple.callkit.call-directory",
                        "NSExtensionPrincipalClass": "CallDir.CallDirectoryHandler"}
                (bundle / "Info.plist").write_bytes(plistlib.dumps(info))
                (bundle / "binary").write_bytes(b"fixture only, not executable")
                (bundle / "HarnessFixture.json").write_text(json.dumps(fixture))
            probe.verify_bundle(app, fixture)
            with self.assertRaises(ValueError):
                probe.verify_bundle(app, fixture, signed=True)
            (ext / "HarnessFixture.json").write_text("{}")
            with self.assertRaises(ValueError):
                probe.verify_bundle(app, fixture)
            (ext / "HarnessFixture.json").write_text(json.dumps(fixture))
            app_info = plistlib.loads((app / "Info.plist").read_bytes())
            app_info["CFBundleVersion"] = "2"
            (app / "Info.plist").write_bytes(plistlib.dumps(app_info))
            with self.assertRaisesRegex(ValueError, "version mismatch"):
                probe.verify_bundle(app, fixture)
            del app_info["CFBundleVersion"]
            (app / "Info.plist").write_bytes(plistlib.dumps(app_info))
            with self.assertRaisesRegex(ValueError, "version is missing"):
                probe.verify_bundle(app, fixture)

    def test_installer_targets_only_the_explicit_device(self):
        fixture = probe.fixture_for("empty", "test")
        with tempfile.TemporaryDirectory() as tmp:
            app = Path(tmp).resolve() / "LimitTest.app"
            app.mkdir()
            (app / "HarnessFixture.json").write_text(json.dumps(fixture))
            args = type("Args", (), {"app": str(app), "device": "selected-device"})()
            with patch.object(probe, "OUTPUT", Path(tmp)), \
                    patch.object(probe, "verify_bundle") as verify, \
                    patch.object(probe.subprocess, "run") as run, patch("builtins.print"):
                probe.install(args)
                verify.assert_called_once_with(app, fixture, signed=True)
                self.assertEqual(run.call_count, 2)
                self.assertEqual(run.call_args_list[-1].args[0],
                                 ["xcrun", "devicectl", "device", "install", "app",
                                  "--device", "selected-device", str(app)])
                self.assertNotIn("launch", run.call_args_list[-1].args[0])

    def test_install_rejects_unrelated_path_without_running_tools(self):
        args = type("Args", (), {"app": "/unrelated/Example.app", "device": "selected-device"})()
        with patch.object(probe.subprocess, "run") as run:
            with self.assertRaises(ValueError):
                probe.install(args)
            run.assert_not_called()


if __name__ == "__main__":
    unittest.main()
