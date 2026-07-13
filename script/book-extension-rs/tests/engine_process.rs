use std::{path::PathBuf, thread, time::Duration};

use book_extension_runtime::engine::{EngineOptions, UsiEngine};
use book_extension_runtime::usi::PositionRoot;

fn fake_engine() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_fake-usi-engine"))
}

#[test]
fn process_engine_initializes_and_searches_with_full_history() {
    let mut engine = UsiEngine::start(
        &fake_engine(),
        EngineOptions {
            hash_mb: 16,
            threads: 1,
            multipv: 2,
            extra: vec![],
        },
    )
    .expect("engine starts");

    let result = engine
        .search(&PositionRoot::Startpos, &["7g7f", "3c3d"], 100)
        .expect("search succeeds");

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].move_usi, "2g2f");
    assert_eq!(result[0].response, "8c8d");
    engine.close().unwrap();
}

#[test]
fn process_engine_searchmove_accepts_bound_and_ponder_response() {
    let mut engine = UsiEngine::start(
        &fake_engine(),
        EngineOptions {
            hash_mb: 16,
            threads: 1,
            multipv: 1,
            extra: vec![],
        },
    )
    .unwrap();

    let result = engine
        .search_move(
            &PositionRoot::Sfen(
                "lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1".to_owned(),
            ),
            &[],
            200,
            "7g7f",
        )
        .unwrap();

    assert_eq!(result.eval_cp, 135);
    assert_eq!(result.response, "3c3d");
    engine.close().unwrap();
}

#[test]
fn malformed_bestmove_is_reported_instead_of_waiting_forever() {
    let mut engine = UsiEngine::start(
        &fake_engine(),
        EngineOptions {
            hash_mb: 16,
            threads: 1,
            multipv: 1,
            extra: vec![],
        },
    )
    .unwrap();
    let error = engine
        .search(&PositionRoot::Startpos, &[], 991)
        .unwrap_err();
    assert!(error.to_string().contains("malformed USI bestmove"));
    engine.close().unwrap();
}
#[test]
fn terminal_bestmoves_resign_win_and_none_produce_no_fake_pv() {
    let options = EngineOptions {
        hash_mb: 16,
        threads: 1,
        multipv: 1,
        extra: vec![],
    };
    for nodes in [994, 993, 992] {
        let mut engine = UsiEngine::start(&fake_engine(), options.clone()).unwrap();
        assert!(
            engine
                .search(&PositionRoot::Startpos, &[], nodes)
                .unwrap()
                .is_empty(),
            "nodes={nodes}"
        );
        engine.close().unwrap();
    }
}
#[test]
fn engine_restarts_after_unexpected_eof_without_replacing_stop_handle() {
    let options = EngineOptions {
        hash_mb: 16,
        threads: 1,
        multipv: 1,
        extra: vec![],
    };
    let mut engine = UsiEngine::start(&fake_engine(), options.clone()).unwrap();
    let stop = engine.stop_handle();

    assert!(engine.search(&PositionRoot::Startpos, &[], 996).is_err());
    engine.restart(&fake_engine(), options).unwrap();
    assert_eq!(
        engine.search(&PositionRoot::Startpos, &[], 100).unwrap()[0].move_usi,
        "7g7f"
    );
    assert!(!stop.stop().unwrap());
    engine.close().unwrap();
}
#[test]
fn stop_handle_interrupts_an_active_search_only() {
    let mut engine = UsiEngine::start(
        &fake_engine(),
        EngineOptions {
            hash_mb: 16,
            threads: 1,
            multipv: 1,
            extra: vec![],
        },
    )
    .unwrap();
    let stop = engine.stop_handle();
    assert!(!stop.stop().unwrap());

    let worker = thread::spawn(move || {
        let result = engine.search(&PositionRoot::Startpos, &[], 999);
        (engine, result)
    });
    thread::sleep(Duration::from_millis(100));
    assert!(stop.stop().unwrap());
    let (mut engine, result) = worker.join().unwrap();
    assert!(result.unwrap().is_empty());
    engine.close().unwrap();
}
