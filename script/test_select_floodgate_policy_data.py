from __future__ import annotations

import csv
import pathlib
import sys
import tempfile
import textwrap
import unittest

import yaml

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from select_floodgate_policy_data import (
    build_position_command,
    load_rating_snapshots,
    parse_csa_game,
    select_games,
    write_outputs,
)


class SelectFloodgatePolicyDataTest(unittest.TestCase):
    def test_parse_csa_game_converts_basic_opening_to_usi(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            csa_path = pathlib.Path(tmp_dir) / "wdoor+floodgate-300-10F+EngineA+EngineB+20260215120000.csa"
            csa_path.write_text(
                textwrap.dedent(
                    """\
                    N+EngineA
                    N-EngineB
                    PI
                    +
                    +7776FU
                    -3334FU
                    %TORYO
                    """
                ),
                encoding="utf-8",
            )

            game = parse_csa_game(csa_path)

            self.assertEqual(game.black_name, "EngineA")
            self.assertEqual(game.white_name, "EngineB")
            self.assertEqual(game.usi_moves, ["7g7f", "3c3d"])
            self.assertEqual(build_position_command(game.usi_moves), "position startpos moves 7g7f 3c3d")

    def test_load_rating_snapshots_ignores_short_term_yaml(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            rating_dir = pathlib.Path(tmp_dir)
            (rating_dir / "players-floodgate-20260101.yaml").write_text(
                yaml.safe_dump([{"name": "EngineA", "rate": 3290}, {"name": "EngineB", "rate": 3400}]),
                encoding="utf-8",
            )
            (rating_dir / "players-floodgate14-20260101.yaml").write_text(
                yaml.safe_dump([{"name": "EngineA", "rate": 5000}]),
                encoding="utf-8",
            )

            snapshots = load_rating_snapshots(rating_dir)

            self.assertEqual(len(snapshots), 1)
            self.assertEqual(snapshots[0].ratings["EngineA"], 3290)

    def test_load_rating_snapshots_parses_long_term_html(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            rating_dir = pathlib.Path(tmp_dir)
            (rating_dir / "players-floodgate-20250428.html").write_text(
                textwrap.dedent(
                    """\
                    <html><body>
                    <table><tbody>
                      <tr class="default">
                        <td class="name"><a href="/player/A">EngineA</a></td>
                        <td class="rate"><span> 3401</span></td>
                      </tr>
                      <tr class="default">
                        <td class="name"><a href="/player/B">EngineB</a></td>
                        <td class="rate"><span> 3555</span></td>
                      </tr>
                    </tbody></table>
                    </body></html>
                    """
                ),
                encoding="utf-8",
            )
            (rating_dir / "players-floodgate14-20250428.html").write_text(
                textwrap.dedent(
                    """\
                    <html><body>
                    <table><tbody>
                      <tr class="default">
                        <td class="name"><a href="/player/A">EngineA</a></td>
                        <td class="rate"><span> 9999</span></td>
                      </tr>
                    </tbody></table>
                    </body></html>
                    """
                ),
                encoding="utf-8",
            )

            snapshots = load_rating_snapshots(rating_dir)

            self.assertEqual(len(snapshots), 1)
            self.assertEqual(snapshots[0].ratings["EngineA"], 3401)
            self.assertEqual(snapshots[0].ratings["EngineB"], 3555)

    def test_select_games_uses_latest_snapshot_not_after_game_date(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = pathlib.Path(tmp_dir)
            rating_dir = root / "ratings"
            csa_dir = root / "csa" / "2026" / "02" / "15"
            rating_dir.mkdir(parents=True)
            csa_dir.mkdir(parents=True)

            (rating_dir / "players-floodgate-20260101.yaml").write_text(
                yaml.safe_dump(
                    [
                        {"name": "EngineA", "rate": 3290},
                        {"name": "EngineB", "rate": 3400},
                    ]
                ),
                encoding="utf-8",
            )
            (rating_dir / "players-floodgate-20260201.yaml").write_text(
                yaml.safe_dump(
                    [
                        {"name": "EngineA", "rate": 3310},
                        {"name": "EngineB", "rate": 3410},
                    ]
                ),
                encoding="utf-8",
            )

            csa_path = csa_dir / "wdoor+floodgate-300-10F+EngineA+EngineB+20260215120000.csa"
            csa_path.write_text(
                textwrap.dedent(
                    """\
                    N+EngineA
                    N-EngineB
                    PI
                    +7776FU
                    -3334FU
                    +2726FU
                    -8384FU
                    %TORYO
                    """
                ),
                encoding="utf-8",
            )

            snapshots = load_rating_snapshots(rating_dir)
            selected = select_games(
                csa_dir=csa_dir,
                snapshots=snapshots,
                min_rating=3300,
                min_plies=0,
                allow_nonstandard_result=False,
                limit=0,
            )

            self.assertEqual(len(selected), 1)
            self.assertEqual(selected[0][1], 3310)
            self.assertEqual(selected[0][2], 3410)

    def test_write_outputs_emits_position_lines_and_manifest(self) -> None:
        with tempfile.TemporaryDirectory() as tmp_dir:
            root = pathlib.Path(tmp_dir)
            rating_dir = root / "ratings"
            csa_dir = root / "csa"
            output_path = root / "selected.txt"
            manifest_path = root / "selected.csv"
            rating_dir.mkdir()
            csa_dir.mkdir()

            (rating_dir / "players-floodgate-20260201.yaml").write_text(
                yaml.safe_dump(
                    [
                        {"name": "EngineA", "rate": 3500},
                        {"name": "EngineB", "rate": 3400},
                    ]
                ),
                encoding="utf-8",
            )
            csa_path = csa_dir / "wdoor+floodgate-300-10F+EngineA+EngineB+20260215120000.csa"
            csa_path.write_text(
                textwrap.dedent(
                    """\
                    N+EngineA
                    N-EngineB
                    PI
                    +7776FU
                    -3334FU
                    %TORYO
                    """
                ),
                encoding="utf-8",
            )

            snapshots = load_rating_snapshots(rating_dir)
            selected = select_games(
                csa_dir=csa_dir,
                snapshots=snapshots,
                min_rating=3300,
                min_plies=0,
                allow_nonstandard_result=False,
                limit=0,
            )
            write_outputs(selected, output_path, manifest_path)

            self.assertEqual(output_path.read_text(encoding="utf-8").strip(), "position startpos moves 7g7f 3c3d")

            with manifest_path.open("r", encoding="utf-8", newline="") as fh:
                rows = list(csv.DictReader(fh))
            self.assertEqual(len(rows), 1)
            self.assertEqual(rows[0]["black_name"], "EngineA")
            self.assertEqual(rows[0]["white_name"], "EngineB")
            self.assertEqual(rows[0]["plies"], "2")


if __name__ == "__main__":
    unittest.main()
