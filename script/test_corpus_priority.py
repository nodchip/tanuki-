from __future__ import annotations

import pathlib
import sys
import unittest

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

from corpus_priority import (
    POLICY_VERSION,
    PriorityFacts,
    ProgressiveWidth,
    SiteBudget,
    SiteNodeBudget,
    priority_tuple,
    recency_bucket,
)


class CorpusPriorityTest(unittest.TestCase):
    def test_three_calendar_years_share_a_bucket_and_newer_bucket_wins(self) -> None:
        self.assertEqual(recency_bucket(2024), recency_bucket(2026))
        self.assertGreater(recency_bucket(2027), recency_bucket(2026))

    def test_tournament_stage_and_mover_rank_precede_opponent_rank(self) -> None:
        strong_stage = PriorityFacts(year=2025, stage_tier=3, mover_percentile=.1, opponent_percentile=0)
        strong_rank = PriorityFacts(year=2025, stage_tier=2, mover_percentile=.9, opponent_percentile=1)
        self.assertGreater(priority_tuple(strong_stage), priority_tuple(strong_rank))

    def test_unreliable_floodgate_rating_does_not_outrank_reliable_rating(self) -> None:
        reliable = PriorityFacts(year=2025, rating_reliability=3, anchor_margin=50, rating_percentile=.6)
        disconnected = PriorityFacts(year=2025, rating_reliability=1, anchor_margin=1000, rating_percentile=1)
        self.assertGreater(priority_tuple(reliable), priority_tuple(disconnected))

    def test_site_budget_uses_40_40_20_and_redistributes_empty_queue(self) -> None:
        budget = SiteBudget()
        self.assertEqual(budget.allocate(100, {"wcsc", "denryu", "floodgate"}), {"wcsc": 40, "denryu": 40, "floodgate": 20})
        self.assertEqual(budget.allocate(100, {"wcsc", "floodgate"}), {"wcsc": 67, "floodgate": 33})

    def test_node_budget_orders_underused_sites_and_records_nodes(self) -> None:
        budget = SiteNodeBudget()
        self.assertEqual(budget.order()[0], "wcsc")
        budget.record("wcsc", 40)
        budget.record("denryu", 40)
        self.assertEqual(budget.order()[0], "floodgate")
        budget.record("floodgate", 20)
        self.assertEqual(budget.snapshot(), {"wcsc": 40, "denryu": 40, "floodgate": 20})
    def test_progressive_width_increments_only_after_saturated_window(self) -> None:
        width = ProgressiveWidth(width=1, window_size=3)
        width.observe(added=False, pending_or_running=True)
        width.observe(added=False, pending_or_running=False)
        self.assertFalse(width.observe(added=False, pending_or_running=False))
        self.assertEqual(width.width, 1)
        self.assertTrue(width.observe(added=False, pending_or_running=False))
        self.assertEqual(width.width, 2)
        self.assertEqual(width.rollouts_since_addition, 0)

    def test_policy_is_explicitly_versioned(self) -> None:
        self.assertTrue(POLICY_VERSION)


if __name__ == "__main__":
    unittest.main()