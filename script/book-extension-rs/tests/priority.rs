use book_extension_runtime::priority::{
    PriorityFacts, SiteNodeBudget, priority_tuple, recency_bucket,
};

#[test]
fn three_calendar_year_bucket_and_tournament_order_match_python() {
    assert_eq!(recency_bucket(2024).unwrap(), recency_bucket(2026).unwrap());
    assert!(recency_bucket(2027).unwrap() > recency_bucket(2026).unwrap());
    let stage = PriorityFacts {
        year: 2025,
        stage_tier: 3,
        mover_percentile: 0.1,
        ..Default::default()
    };
    let rank = PriorityFacts {
        year: 2025,
        stage_tier: 2,
        mover_percentile: 0.9,
        opponent_percentile: 1.0,
        ..Default::default()
    };
    assert!(priority_tuple(&stage).unwrap() > priority_tuple(&rank).unwrap());
}

#[test]
fn weighted_actual_nodes_make_floodgate_next_after_40_40_usage() {
    let mut budget =
        SiteNodeBudget::new([("wcsc", 40), ("denryu", 40), ("floodgate", 20)]).unwrap();
    assert_eq!(budget.order()[0], "wcsc");
    budget.record("wcsc", 40).unwrap();
    budget.record("denryu", 40).unwrap();
    assert_eq!(budget.order()[0], "floodgate");
    budget.record("floodgate", 20).unwrap();
    assert_eq!(
        budget.snapshot(),
        [("wcsc", 40), ("denryu", 40), ("floodgate", 20)]
    );
}
