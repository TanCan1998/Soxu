use soxu_git::{StatusEntry, parse_status};

fn ordinary(status: &str, submodule: &str, path: &str) -> Vec<u8> {
    format!(
        "1 {status} {submodule} 100644 100644 100644 {} {} {path}\0",
        "a".repeat(40),
        "b".repeat(40)
    )
    .into_bytes()
}

#[test]
fn preserves_unquoted_paths_headers_and_submodule_flags() {
    let mut bytes =
        b"# branch.oid (initial)\0# branch.head main\0# future.name arbitrary value\0".to_vec();
    bytes.extend(ordinary(".M", "SCMU", " 空 格/中文\n换行\t文件 "));
    bytes.extend(b"? raw-\xff\0! ignored\0");
    let snapshot = parse_status(&bytes).expect("解析原始字节");
    assert_eq!(
        snapshot.header(b"future.name"),
        Some(b"arbitrary value".as_slice())
    );
    let StatusEntry::Ordinary(change) = &snapshot.entries[0] else {
        panic!("应为普通变更");
    };
    assert_eq!(
        change.path.as_bytes(),
        " 空 格/中文\n换行\t文件 ".as_bytes()
    );
    assert!(
        change.submodule.is_submodule
            && change.submodule.commit_changed
            && change.submodule.tracked_changes
            && change.submodule.untracked_changes
    );
    let StatusEntry::Untracked(path) = &snapshot.entries[1] else {
        panic!("应为未跟踪路径");
    };
    assert_eq!(path.as_bytes(), b"raw-\xff");
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        assert_eq!(
            path.to_path_buf()
                .expect("Unix 原始路径")
                .as_os_str()
                .as_bytes(),
            b"raw-\xff"
        );
    }
    #[cfg(windows)]
    assert!(path.to_path_buf().is_err());
    assert!(matches!(snapshot.entries[2], StatusEntry::Ignored(_)));
}

#[test]
fn consumes_rename_source_as_path_not_record() {
    let bytes = format!(
        "2 R. N... 100644 100644 100644 {} {} R100 new name\n中文\0? source name\0? next\0",
        "a".repeat(64),
        "b".repeat(64)
    );
    let snapshot = parse_status(bytes.as_bytes()).expect("解析 SHA-256 重命名");
    assert_eq!(snapshot.entries.len(), 2);
    let StatusEntry::Renamed(rename) = &snapshot.entries[0] else {
        panic!("应为重命名");
    };
    assert_eq!(rename.similarity, 100);
    assert_eq!(rename.original_path.as_bytes(), b"? source name");
    assert_eq!(rename.change.path.as_bytes(), "new name\n中文".as_bytes());
}

#[test]
fn parses_all_unmerged_pairs_and_copy() {
    for pair in ["DD", "AU", "UD", "UA", "DU", "AA", "UU"] {
        let bytes = format!(
            "u {pair} N... 100644 100644 100644 100644 {0} {0} {0} conflicted\0",
            "0".repeat(40)
        );
        assert!(matches!(
            parse_status(bytes.as_bytes()).expect("冲突记录").entries[0],
            StatusEntry::Unmerged(_)
        ));
    }
    let bytes = format!(
        "2 C. N... 100644 100644 100644 {0} {0} C75 copied\0source\0",
        "1".repeat(40)
    );
    assert!(matches!(
        parse_status(bytes.as_bytes()).expect("复制记录").entries[0],
        StatusEntry::Renamed(_)
    ));
}

#[test]
fn rejects_malformed_input_without_partial_results() {
    let mut malformed = vec![
        b"? missing-nul".to_vec(),
        b"\0".to_vec(),
        b"? \0".to_vec(),
        b"? ../escape\0".to_vec(),
        b"? /absolute\0".to_vec(),
        b"? a//b\0".to_vec(),
        b"? a/./b\0".to_vec(),
        b"x unknown\0".to_vec(),
        b"1 incomplete\0".to_vec(),
        b"# branch.head main\0# branch.head other\0".to_vec(),
        b"# branch.oid abcd\0".to_vec(),
        b"# branch.ab +1 bad\0".to_vec(),
        ordinary("ZZ", "N...", "file"),
        ordinary("..", "N...", "file"),
        ordinary("UU", "N...", "file"),
        ordinary("R.", "N...", "file"),
        ordinary(".M", "NCMU", "file"),
        ordinary(".M", "SXYZ", "file"),
    ];
    let rename = format!(
        "2 R. N... 100644 100644 100644 {0} {0} R100 new\0",
        "a".repeat(40)
    );
    malformed.push(rename.as_bytes().to_vec());
    malformed.push(format!("{rename}\0").into_bytes());
    malformed.push(format!("{}source\0", rename.replace("R100", "R101")).into_bytes());
    malformed.push(format!("{}source\0", rename.replace("R100", "C50")).into_bytes());
    malformed.push(
        ordinary(".M", "N...", "file")
            .into_iter()
            .enumerate()
            .map(|(index, byte)| if index == 13 { b'9' } else { byte })
            .collect(),
    );
    for bytes in malformed {
        assert!(
            parse_status(&bytes).is_err(),
            "非法输入不得被接受：{bytes:?}"
        );
    }
    assert!(parse_status(b"").expect("空状态").entries.is_empty());
}

#[test]
fn windows_conversion_never_normalizes_special_identity() {
    for name in [
        "NUL",
        "con.txt",
        "trailing.",
        "trailing ",
        "a\\b",
        "a:b",
        "line\nname",
    ] {
        let bytes = format!("? {name}\0");
        let snapshot = parse_status(bytes.as_bytes()).expect("保留跨平台路径");
        let StatusEntry::Untracked(path) = &snapshot.entries[0] else {
            panic!("未跟踪路径");
        };
        assert_eq!(path.as_bytes(), name.as_bytes());
        #[cfg(windows)]
        assert!(path.to_path_buf().is_err());
    }
}
