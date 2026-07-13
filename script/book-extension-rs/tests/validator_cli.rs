use std::process::Command;

#[test]
fn validator_binary_reports_counts_and_nonzero_for_invalid_book() {
    let directory = tempfile::tempdir().unwrap();
    let valid = directory.path().join("valid.db");
    std::fs::write(
        &valid,
        "#YANEURAOU-DB2016 1.00\nsfen lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1\n7g7f 3c3d 0 1 0\n",
    )
    .unwrap();
    let valid_output = Command::new(env!("CARGO_BIN_EXE_book-validator"))
        .arg(&valid)
        .output()
        .unwrap();
    assert!(valid_output.status.success());
    let stdout = String::from_utf8(valid_output.stdout).unwrap();
    assert!(
        stdout.contains("positions=1 entries=1 issues=0"),
        "{stdout}"
    );

    let invalid = directory.path().join("invalid.db");
    std::fs::write(
        &invalid,
        "#YANEURAOU-DB2016 1.00\nsfen lnsgkgsnl/1r5b1/ppppppppp/9/9/9/PPPPPPPPP/1B5R1/LNSGKGSNL b - 1\n7g7e none 0 1 0\n",
    )
    .unwrap();
    let invalid_output = Command::new(env!("CARGO_BIN_EXE_book-validator"))
        .arg(&invalid)
        .output()
        .unwrap();
    assert_eq!(invalid_output.status.code(), Some(2));
    let stdout = String::from_utf8(invalid_output.stdout).unwrap();
    assert!(stdout.contains("issues=1"), "{stdout}");
    assert!(stdout.contains("illegal-move"), "{stdout}");
}
