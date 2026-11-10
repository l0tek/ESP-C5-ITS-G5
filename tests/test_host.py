"""Integration tests for the recorder; run with python3 tests/test_host.py."""
import pathlib
import signal
import struct
import subprocess
import tempfile
import time
import unittest

ROOT = pathlib.Path(__file__).resolve().parents[1]
FRAME = bytes([8]) + bytes(23)


class RecorderTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.build = tempfile.TemporaryDirectory(prefix="cits-host-build-")
        cls.binary = pathlib.Path(cls.build.name) / "cits-pcap"
        subprocess.run(
            ["rustc", "--edition=2021", "-O", str(ROOT / "host/src/main.rs"),
             "-o", str(cls.binary)], check=True, cwd=ROOT,
        )

    @classmethod
    def tearDownClass(cls):
        cls.build.cleanup()

    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="cits-host-test-")
        self.addCleanup(self.directory.cleanup)
        self.output = pathlib.Path(self.directory.name) / "capture.pcap"

    def run_recorder(self, source="-", text=""):
        return subprocess.run(
            [str(self.binary), str(source), str(self.output)],
            input=text, text=True, capture_output=True, timeout=5,
        )

    def test_pcap_records_and_invalid_input(self):
        result = self.run_recorder(text=(
            "CITS-READY\nCITS:" + FRAME.hex() + "\nCITS:xyz\nCITS:00\n"
        ))
        self.assertEqual(result.returncode, 0, result.stderr)
        data = self.output.read_bytes()
        self.assertEqual(struct.unpack("<IHHIIII", data[:24]),
                         (0xA1B2C3D4, 2, 4, 0, 0, 65535, 105))
        self.assertEqual(len(data), 64)
        self.assertEqual(struct.unpack("<II", data[32:40]), (24, 24))
        self.assertEqual(data[40:], FRAME)
        self.assertIn("Frames written: 1, invalid: 2", result.stderr)

    def test_sigint_preserves_processed_frame(self):
        proc = subprocess.Popen(
            [str(self.binary), "-", str(self.output)],
            stdin=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        try:
            proc.stdin.write("CITS:" + FRAME.hex() + "\n")
            proc.stdin.flush()
            deadline = time.monotonic() + 5
            while not self.output.exists() or self.output.stat().st_size < 64:
                if time.monotonic() >= deadline or proc.poll() is not None:
                    self.fail("Recorder did not flush the frame while input stayed open")
                time.sleep(0.01)
            proc.send_signal(signal.SIGINT)
            proc.wait(timeout=5)
            self.assertEqual(self.output.read_bytes()[40:], FRAME)
        finally:
            if proc.poll() is None:
                proc.kill()
                proc.wait(timeout=5)
            proc.stdin.close()
            proc.stderr.close()

    def test_existing_output_is_preserved(self):
        self.output.write_bytes(b"existing capture")
        self.assertNotEqual(self.run_recorder().returncode, 0)
        self.assertEqual(self.output.read_bytes(), b"existing capture")

    def test_invalid_input_does_not_create_output(self):
        self.assertNotEqual(self.run_recorder(self.output.parent / "missing").returncode, 0)
        self.assertFalse(self.output.exists())

    def test_same_input_output_is_preserved(self):
        self.output.write_bytes(b"source log")
        self.assertNotEqual(self.run_recorder(self.output).returncode, 0)
        self.assertEqual(self.output.read_bytes(), b"source log")


if __name__ == "__main__":
    unittest.main(verbosity=2)
