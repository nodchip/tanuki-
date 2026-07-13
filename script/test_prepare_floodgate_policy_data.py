from __future__ import annotations

import pathlib
import sys
import unittest
from http.client import RemoteDisconnected

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from prepare_floodgate_policy_data import (
    archive_url_for_year,
    classify_index_links,
    extract_links,
    format_progress_label,
    is_retryable_download_error,
    list_rating_links,
    extracted_csa_root_for_year,
)


class PrepareFloodgatePolicyDataTest(unittest.TestCase):
    def test_extract_links_reads_directory_index(self) -> None:
        html = """\
        <!DOCTYPE html>
        <html><body>
        <ul>
          <li><a href="12">12</a></li>
          <li><a href="prating">prating</a></li>
          <li><a href="wdoor+floodgate-300-10F+EngineA+EngineB+20250427233008.csa">csa</a></li>
        </ul>
        </body></html>
        """

        links = extract_links(html)

        self.assertEqual(
            links,
            [
                "12",
                "prating",
                "wdoor+floodgate-300-10F+EngineA+EngineB+20250427233008.csa",
            ],
        )

    def test_classify_index_links_separates_directories_and_csa(self) -> None:
        directories, csa_files = classify_index_links(
            [
                "/shogi",
                "/shogi/shogi.css",
                "12",
                "prating",
                "27",
                "wdoor+floodgate-300-10F+EngineA+EngineB+20250427233008.csa",
                "wdoor+floodgate-300-10F+EngineA+EngineB+20250427233008.html",
            ]
        )

        self.assertEqual(directories, ["12", "27"])
        self.assertEqual(csa_files, ["wdoor+floodgate-300-10F+EngineA+EngineB+20250427233008.csa"])

    def test_list_rating_links_uses_long_term_yaml_only(self) -> None:
        html = """\
        <!DOCTYPE html>
        <html><body>
        <ul>
          <li><a href="players-floodgate-20250428.yaml">players-floodgate-20250428.yaml</a></li>
          <li><a href="players-floodgate-20250428.html">players-floodgate-20250428</a></li>
          <li><a href="players-floodgate14-20250428.yaml">players-floodgate14-20250428.yaml</a></li>
          <li><a href="players-floodgate-20231231.yaml">players-floodgate-20231231.yaml</a></li>
        </ul>
        </body></html>
        """

        links = list_rating_links(html, {2025})

        self.assertEqual(links, ["players-floodgate-20250428.yaml"])

    def test_format_progress_label_includes_position_and_context(self) -> None:
        label = format_progress_label("kifu", 3, 12, "2024/05/07")

        self.assertEqual(label, "[kifu] 3/12 2024/05/07")

    def test_is_retryable_download_error_matches_network_disconnects(self) -> None:
        self.assertTrue(is_retryable_download_error(RemoteDisconnected("closed")))
        self.assertFalse(is_retryable_download_error(ValueError("bad input")))

    def test_archive_helpers_build_expected_paths(self) -> None:
        self.assertEqual(
            archive_url_for_year(2024),
            "https://wdoor.c.u-tokyo.ac.jp/shogi/archive/wdoor2024.7z",
        )
        self.assertEqual(
            extracted_csa_root_for_year(pathlib.Path("C:/tmp/work"), 2024),
            pathlib.Path("C:/tmp/work/archive/2024"),
        )


if __name__ == "__main__":
    unittest.main()
