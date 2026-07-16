use book_extension_runtime::priority::{
    FloodgateStrength, OfficialStrength, PriorityFacts, QualityFacts, SiteNodeBudget,
    encode_priority, floodgate_strength_tier, official_strength_tier, priority_tuple,
    quality_decision, recency_bucket,
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

#[test]
fn fixed_width_priority_encoding_matches_python_bytes() {
    let facts = PriorityFacts {
        year: 2026,
        stage_tier: 2,
        mover_percentile: 0.75,
        opponent_stage_tier: 1,
        opponent_percentile: 0.25,
        rating_reliability: 2,
        anchor_margin: 123.5,
        rating_percentile: 0.9,
        recent_occurrences: 12,
        occurrences: 34,
        exact_time: 1_783_900_800.0,
    };
    let tuple = priority_tuple(&facts).unwrap();
    let encoded = encode_priority(&tuple).unwrap();
    assert_eq!(
        hex(&encoded),
        "000000e8d4a51000000000e8d4c39480000000e8d4b081b0000000e8d4b45240000000e8d4a8e090000000e8d4c39480000000e8dc0185e0000000e8d4b2cba0000000e8d55c2b00000000e8d6abdc800006575b9a24b000"
    );
}

#[test]
fn old_champion_precedes_current_lower_ai() {
    let old_champion = quality_decision(
        2026,
        &QualityFacts {
            event_year: 2023,
            strength_tier: 0,
        },
    )
    .unwrap();
    let current_lower_ai = quality_decision(
        2026,
        &QualityFacts {
            event_year: 2026,
            strength_tier: 3,
        },
    )
    .unwrap();

    assert_eq!(old_champion.age_distance, 1);
    assert_eq!(old_champion.quality_band, 2);
    assert_eq!(current_lower_ai.quality_band, 3);
    assert!(old_champion.quality_band < current_lower_ai.quality_band);
}

#[test]
fn official_and_floodgate_tiers_follow_policy() {
    assert_eq!(
        official_strength_tier(OfficialStrength::SingleStage {
            rank: 1,
            participants: 16,
        }),
        0
    );
    assert_eq!(
        official_strength_tier(OfficialStrength::SingleStage {
            rank: 4,
            participants: 16,
        }),
        2
    );
    assert_eq!(
        official_strength_tier(OfficialStrength::TopStage {
            rank: 8,
            participants: 8,
        }),
        2
    );
    assert_eq!(
        official_strength_tier(OfficialStrength::OneStageBelow {
            rank: 2,
            participants: 8,
        }),
        3
    );
    assert_eq!(
        official_strength_tier(OfficialStrength::TwoOrMoreStagesBelow),
        4
    );
    assert_eq!(
        floodgate_strength_tier(FloodgateStrength {
            high_reliability: true,
            connected_to_anchor: true,
            enough_effective_games: true,
            snapshot_percentile: 0.99,
        }),
        1
    );
    assert_eq!(
        floodgate_strength_tier(FloodgateStrength {
            high_reliability: true,
            connected_to_anchor: true,
            enough_effective_games: true,
            snapshot_percentile: 0.95,
        }),
        2
    );
    assert_eq!(
        floodgate_strength_tier(FloodgateStrength {
            high_reliability: false,
            connected_to_anchor: true,
            enough_effective_games: true,
            snapshot_percentile: 1.0,
        }),
        4
    );
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|value| format!("{value:02x}")).collect()
}
