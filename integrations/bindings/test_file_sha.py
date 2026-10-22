import hashlib
import tempfile
import unittest
from pathlib import Path
from verify_candidate import file_sha

class FileHashTests(unittest.TestCase):
    def test_empty_partial_and_multiple_chunks(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'artifact'
            for length in (0,1,1048575,1048576,1048577,3145765):
                with self.subTest(length=length):
                    payload=(bytes(range(256)) * ((length+255)//256))[:length]
                    path.write_bytes(payload)
                    self.assertEqual(file_sha(path),hashlib.sha256(payload).hexdigest())

    def test_changed_bytes_same_length_are_detected(self):
        with tempfile.TemporaryDirectory() as directory:
            path=Path(directory)/'artifact';path.write_bytes(b'abc')
            first=file_sha(path);path.write_bytes(b'abd')
            self.assertNotEqual(first,file_sha(path))

    def test_missing_file_rejected(self):
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(FileNotFoundError):file_sha(Path(directory)/'absent')

if __name__=='__main__':unittest.main()
