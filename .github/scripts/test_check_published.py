import io
import http.client
import unittest
import urllib.error
from unittest.mock import patch

from check_published import check, index_path


class PublishedTests(unittest.TestCase):
    def test_index_paths(self):
        for crate, expected in [("A", "1/a"), ("ab", "2/ab"), ("abc", "3/a/abc"), ("salvo", "sa/lv/salvo")]:
            self.assertEqual(index_path(crate), expected)

    def test_first_last_and_absent_versions_in_large_index(self):
        data = b'{"vers":"0.1.0"}\n' + b'{"vers":"0.2.0"}\n' * 100_000 + b'{"vers":"1.1.1"}\n'
        for version, expected in [("0.1.0", 0), ("1.1.1", 0), ("9.9.9", 1)]:
            response = io.BytesIO(data)
            with patch("urllib.request.urlopen", return_value=response):
                self.assertEqual(check("salvo", version), expected)
            self.assertTrue(response.closed)

    def test_transfer_or_parse_failure_is_not_absence(self):
        for error in [OSError("connection lost"), http.client.IncompleteRead(b"partial"), urllib.error.HTTPError("url", 503, "unavailable", {}, None)]:
            with patch("urllib.request.urlopen", side_effect=error):
                self.assertEqual(check("salvo", "1"), 2)
        with patch("urllib.request.urlopen", return_value=io.BytesIO(b'{"vers":"1"}\ntruncated')):
            self.assertEqual(check("salvo", "1"), 2)

    def test_missing_crate_is_absent(self):
        with patch("urllib.request.urlopen", side_effect=urllib.error.HTTPError("url", 404, "missing", {}, None)):
            self.assertEqual(check("new-crate", "1"), 1)


if __name__ == "__main__":
    unittest.main()
