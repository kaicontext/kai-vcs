//! Reproduce golden vectors without linking the production kernel.
use kai_object_reference::{
    Payload, Record,
    Value::{self, *},
    encode, id,
};

fn m(v: Vec<(u64, Value)>) -> Value {
    Map(v.into_iter().collect())
}
fn b(v: &[u8]) -> Value {
    Bytes(v.to_vec())
}
fn r(byte: u8) -> Value {
    Array(vec![UInt(1), UInt(1), b(&[byte; 32])])
}
fn entry(name: &[u8], kind: u64, target: Value) -> Value {
    m(vec![(0, b(name)), (1, UInt(kind)), (2, target)])
}
fn person() -> Value {
    m(vec![
        (0, b(b"A\xff")),
        (1, b(b"a@example.test")),
        (2, Negative(0)),
        (3, Negative(89)),
    ])
}
fn revision(parents: Vec<Value>, change: u8) -> Value {
    m(vec![
        (0, r(0x22)),
        (1, Array(parents)),
        (2, b(&[change; 16])),
        (3, person()),
        (4, person()),
        (5, b(b"message\0\xff\r\n")),
    ])
}

fn conflict_version(label: &[u8], states: Vec<Value>, source: Option<u8>) -> Value {
    let mut fields = vec![(0, b(label)), (1, Array(states))];
    if let Some(source) = source {
        fields.push((2, r(source)));
    }
    m(fields)
}
fn present(kind: u64, id: u8) -> Value {
    Array(vec![UInt(1), UInt(kind), r(id)])
}
fn missing() -> Value {
    Array(vec![UInt(0)])
}

fn main() {
    let mut vectors = vec![
        (
            "blob-empty",
            Record {
                kind: 1,
                payload: Payload::Raw(vec![]),
            },
        ),
        (
            "blob-hello",
            Record {
                kind: 1,
                payload: Payload::Raw(b"hello\n".to_vec()),
            },
        ),
        (
            "blob-binary",
            Record {
                kind: 1,
                payload: Payload::Raw((0u8..=255).collect()),
            },
        ),
        (
            "tree-empty",
            Record {
                kind: 2,
                payload: Payload::Structured(Array(vec![])),
            },
        ),
        (
            "tree-mixed",
            Record {
                kind: 2,
                payload: Payload::Structured(Array(vec![
                    entry(b"A", 1, r(0x11)),
                    entry(b"a", 2, r(0x11)),
                    entry(b"dir", 4, r(0x22)),
                    entry(b"git", 5, Array(vec![UInt(0), UInt(1), b(&[0x33; 20])])),
                    entry(b"git256", 5, Array(vec![UInt(0), UInt(2), b(&[0x44; 32])])),
                    entry(b"link", 3, r(0x55)),
                    entry(b"\xff", 1, r(0x66)),
                ])),
            },
        ),
        (
            "revision-root",
            Record {
                kind: 3,
                payload: Payload::Structured(revision(vec![], 0x77)),
            },
        ),
        (
            "revision-merge",
            Record {
                kind: 3,
                payload: Payload::Structured(revision(vec![r(0x88), r(0x99)], 0x78)),
            },
        ),
        (
            "chunk-small",
            Record {
                kind: 13,
                payload: Payload::Raw(b"\0\xff\r\n".to_vec()),
            },
        ),
        (
            "large-manifest",
            Record {
                kind: 14,
                payload: Payload::Structured(m(vec![
                    (0, UInt(16 * 1024 * 1024 + 1)),
                    (1, b(&[0xaa; 32])),
                    (2, Array(vec![r(0xbb), r(0xbb), r(0xbb), r(0xbb), r(0xcc)])),
                ])),
            },
        ),
    ];
    vectors.extend([
        (
            "conflict-delete",
            Record {
                kind: 5,
                payload: Payload::Structured(m(vec![
                    (0, UInt(3)),
                    (1, Array(vec![b(b"src/a")])),
                    (
                        2,
                        Array(vec![conflict_version(
                            b"base",
                            vec![present(1, 0x11)],
                            Some(0x22),
                        )]),
                    ),
                    (
                        3,
                        Array(vec![
                            conflict_version(b"ours", vec![missing()], None),
                            conflict_version(b"theirs", vec![present(2, 0x33)], None),
                        ]),
                    ),
                    (4, b(b"diff3/v1")),
                    (5, b(b"conservative/v1")),
                ])),
            },
        ),
        (
            "conflict-rename",
            Record {
                kind: 5,
                payload: Payload::Structured(m(vec![
                    (0, UInt(5)),
                    (1, Array(vec![b(b"a"), b(b"b")])),
                    (
                        2,
                        Array(vec![conflict_version(
                            b"base",
                            vec![present(1, 0x11), missing()],
                            None,
                        )]),
                    ),
                    (
                        3,
                        Array(vec![
                            conflict_version(b"ours", vec![missing(), present(1, 0x11)], None),
                            conflict_version(b"theirs", vec![present(1, 0x33), missing()], None),
                        ]),
                    ),
                    (4, b(b"rename/v1")),
                    (5, b(b"conservative/v1")),
                    (6, Array(vec![Array(vec![b(&[1; 16]), b(b"a"), b(b"b")])])),
                    (7, Array(vec![r(0x44)])),
                ])),
            },
        ),
        (
            "checkpoint-full",
            Record {
                kind: 8,
                payload: Payload::Structured(m(vec![
                    (0, b(&[1; 16])),
                    (1, r(0x22)),
                    (2, r(0x33)),
                    (3, UInt(2)),
                    (4, person()),
                    (
                        5,
                        Array(vec![
                            Array(vec![b(b"ignored"), UInt(2)]),
                            Array(vec![b(b"untracked"), UInt(1)]),
                        ]),
                    ),
                    (6, r(0x44)),
                    (7, Array(vec![Array(vec![b(b"src/a"), r(0x55)])])),
                    (8, r(0x66)),
                    (9, r(0x77)),
                    (10, Array(vec![Array(vec![b(b"selected"), r(0x88)])])),
                ])),
            },
        ),
        (
            "operation-full",
            Record {
                kind: 7,
                payload: Payload::Structured(m(vec![
                    (0, b(&[1; 16])),
                    (1, Array(vec![r(0x11), r(0x22)])),
                    (2, r(0x33)),
                    (3, r(0x44)),
                    (4, b(b"checkpoint")),
                    (5, person()),
                    (6, UInt(100)),
                    (7, UInt(99)),
                    (8, Array(vec![b(&[1; 16]), b(&[2; 16])])),
                    (9, b(&[3; 16])),
                    (10, Array(vec![b(b"peer:receipt/1")])),
                ])),
            },
        ),
        (
            "change-rewrite",
            Record {
                kind: 4,
                payload: Payload::Structured(m(vec![
                    (0, UInt(2)),
                    (1, Array(vec![b(&[1; 16])])),
                    (2, Array(vec![r(0x11)])),
                    (3, Array(vec![r(0x22)])),
                    (4, person()),
                    (5, b(b"amend\0\xff")),
                    (6, r(0x33)),
                ])),
            },
        ),
        (
            "tag-full",
            Record {
                kind: 15,
                payload: Payload::Structured(m(vec![
                    (0, r(0x11)),
                    (1, b(b"release\0\xff")),
                    (2, person()),
                    (3, r(0x22)),
                ])),
            },
        ),
    ]);
    vectors.extend([
        (
            "view-empty",
            Record {
                kind: 6,
                payload: Payload::Structured(m(vec![
                    (0, UInt(0)),
                    (1, Array(vec![])),
                    (2, Array(vec![])),
                    (3, Array(vec![])),
                    (4, Array(vec![])),
                    (5, UInt(0)),
                ])),
            },
        ),
        (
            "view-full",
            Record {
                kind: 6,
                payload: Payload::Structured(m(vec![
                    (0, UInt(24)),
                    (
                        1,
                        Array(vec![
                            Array(vec![b(b"branches/deleted"), UInt(3), Array(vec![UInt(0)])]),
                            Array(vec![
                                b("branches/é".as_bytes()),
                                UInt(7),
                                Array(vec![UInt(2), Array(vec![r(0x11), r(0x22)])]),
                            ]),
                            Array(vec![
                                b(b"remotes/origin/branches/main"),
                                UInt(1),
                                Array(vec![UInt(1), r(0x33)]),
                            ]),
                            Array(vec![
                                b(b"shelves/wip"),
                                UInt(2),
                                Array(vec![UInt(1), r(0x44)]),
                            ]),
                            Array(vec![
                                b(b"tags/v1"),
                                UInt(u64::MAX),
                                Array(vec![UInt(1), r(0x55)]),
                            ]),
                        ]),
                    ),
                    (
                        2,
                        Array(vec![Array(vec![
                            b(&[1; 16]),
                            Array(vec![r(0x11), r(0x22)]),
                        ])]),
                    ),
                    (
                        3,
                        Array(vec![
                            m(vec![(0, b(&[2; 16])), (1, r(0x33)), (2, r(0x44))]),
                            m(vec![(0, b(&[3; 16]))]),
                        ]),
                    ),
                    (
                        4,
                        Array(vec![
                            Array(vec![UInt(6), r(0x66)]),
                            Array(vec![UInt(7), r(0x77)]),
                            Array(vec![UInt(9), r(0x88)]),
                        ]),
                    ),
                    (5, UInt(0)),
                ])),
            },
        ),
    ]);
    vectors.extend([
        (
            "evidence-execution",
            Record {
                kind: 9,
                payload: Payload::Structured(m(vec![
                    (0, r(0x11)),
                    (1, r(0x22)),
                    (2, UInt(1)),
                    (3, UInt(1)),
                    (4, Array(vec![b(b"test"), b(b""), b(b"--all")])),
                    (5, b(b"src")),
                    (6, Array(vec![b(b"inputs/v1"), r(0x33)])),
                    (7, Array(vec![b(b"environment/v1"), r(0x44)])),
                    (8, person()),
                    (9, UInt(2)),
                    (10, UInt(1)),
                    (11, UInt(100)),
                    (12, UInt(99)),
                    (13, Array(vec![Array(vec![b(b"stdout"), r(0x55)])])),
                    (
                        14,
                        m(vec![
                            (0, Array(vec![b(b"unit")])),
                            (1, Array(vec![b(b"network")])),
                            (2, b(b"all/v1")),
                            (3, b(b"")),
                        ]),
                    ),
                    (15, Array(vec![b(b"external service unavailable")])),
                    (16, UInt(0)),
                ])),
            },
        ),
        (
            "attestation-full",
            Record {
                kind: 10,
                payload: Payload::Structured(m(vec![
                    (0, b(b"sshsig")),
                    (1, UInt(1)),
                    (2, b(b"kai-vcs")),
                    (3, UInt(3)),
                    (4, r(0x11)),
                    (5, b(b"revision")),
                    (6, b(b"test-key")),
                    (7, person()),
                    (8, b(b"opaque-signature\0\xff")),
                    (9, Negative(0)),
                    (10, UInt(100)),
                ])),
            },
        ),
        (
            "analysis-partial",
            Record {
                kind: 11,
                payload: Payload::Structured(m(vec![
                    (0, r(0x22)),
                    (1, b(b"analyzer/1")),
                    (2, b(b"resolver/1")),
                    (3, Array(vec![UInt(1), b(&[0x33; 32])])),
                    (4, Array(vec![UInt(1), b(&[0x44; 32])])),
                    (5, b(b"linux-arm64")),
                    (6, UInt(2)),
                    (7, Array(vec![b(b"src/a")])),
                    (8, Array(vec![b(b"src/generated")])),
                    (9, Array(vec![b(b"src/b")])),
                    (10, Array(vec![b(b"dynamic-call")])),
                    (11, Array(vec![r(0x55), r(0x66)])),
                    (12, b(b"unsupported inputs remain")),
                ])),
            },
        ),
    ]);
    for algorithm in [1u64, 2] {
        for (kind, label) in [(1, "blob"), (2, "tree"), (3, "commit"), (4, "tag")] {
            use sha1::Digest;
            let width = if algorithm == 1 { 20 } else { 32 };
            let oid = "22".repeat(width);
            let body=match kind {
                1=>b"hello\0\xff\r\n".to_vec(),
                2=>{let mut b=b"100755 file\0".to_vec();b.extend(vec![0x11;width]);b},
                3=>format!("tree {oid}\nparent {}\nparent {}\nauthor A <a@example.test> 1 +0130\ncommitter B <b@example.test> 2 -0200\nencoding ISO-8859-1\ngpgsig -----BEGIN PGP SIGNATURE-----\n opaque preserved bytes\n -----END PGP SIGNATURE-----\n\nmessage\r\n", "33".repeat(width),"44".repeat(width)).into_bytes(),
                _=>format!("object {oid}\ntype commit\ntag v1\ntagger A <a@example.test> 1 +0000\n\nrelease\n-----BEGIN PGP SIGNATURE-----\nopaque\n-----END PGP SIGNATURE-----\n").into_bytes(),
            };
            let preimage = [
                format!("{label} {}\0", body.len()).as_bytes(),
                body.as_slice(),
            ]
            .concat();
            let digest = if algorithm == 1 {
                sha1::Sha1::digest(&preimage).to_vec()
            } else {
                sha2::Sha256::digest(&preimage).to_vec()
            };
            let name = match (kind, algorithm) {
                (1, 1) => "git-blob-sha1",
                (2, 1) => "git-tree-sha1",
                (3, 1) => "git-commit-sha1",
                (4, 1) => "git-tag-sha1",
                (1, 2) => "git-blob-sha256",
                (2, 2) => "git-tree-sha256",
                (3, 2) => "git-commit-sha256",
                _ => "git-tag-sha256",
            };
            vectors.push((
                name,
                Record {
                    kind: 12,
                    payload: Payload::Structured(m(vec![
                        (0, UInt(kind)),
                        (1, Array(vec![UInt(0), UInt(algorithm), Bytes(digest)])),
                        (2, Bytes(body)),
                    ])),
                },
            ));
        }
    }
    let package_entry =
        |byte, kind, opaque| Array(vec![r(byte), UInt(kind), UInt(100), UInt(opaque)]);
    let package = |mode, roots, inventory, prerequisites| {
        m(vec![
            (0, UInt(mode)),
            (1, Array(roots)),
            (2, Array(inventory)),
            (3, Array(prerequisites)),
            (4, Array(vec![b(b"native/v1")])),
            (5, Array(vec![b(b"opaque/demo")])),
            (6, UInt(1)),
        ])
    };
    vectors.extend([
        (
            "transfer-standalone",
            Record {
                kind: 19,
                payload: Payload::Structured(m(vec![
                    (0, b(&[1; 16])),
                    (
                        1,
                        package(
                            1,
                            vec![Array(vec![r(0x11), UInt(3)])],
                            vec![package_entry(0x11, 3, 1), package_entry(0x22, 500, 2)],
                            vec![],
                        ),
                    ),
                ])),
            },
        ),
        (
            "transfer-incremental",
            Record {
                kind: 19,
                payload: Payload::Structured(m(vec![
                    (0, b(&[1; 16])),
                    (
                        1,
                        package(
                            2,
                            vec![Array(vec![r(0x11), UInt(3)])],
                            vec![package_entry(0x11, 3, 1)],
                            vec![package_entry(0x22, 2, 1)],
                        ),
                    ),
                ])),
            },
        ),
        (
            "backup-initial",
            Record {
                kind: 16,
                payload: Payload::Structured(m(vec![
                    (0, b(&[1; 16])),
                    (1, UInt(0)),
                    (2, r(0x11)),
                    (
                        4,
                        package(1, vec![], vec![package_entry(0x11, 6, 1)], vec![]),
                    ),
                ])),
            },
        ),
        (
            "backup-incremental",
            Record {
                kind: 16,
                payload: Payload::Structured(m(vec![
                    (0, b(&[1; 16])),
                    (1, UInt(42)),
                    (2, r(0x11)),
                    (3, r(0x22)),
                    (
                        4,
                        package(
                            2,
                            vec![Array(vec![r(0x33), UInt(6)])],
                            vec![package_entry(0x11, 6, 1), package_entry(0x22, 7, 1)],
                            vec![package_entry(0x33, 6, 1)],
                        ),
                    ),
                ])),
            },
        ),
    ]);
    vectors.extend([
        (
            "barrier-full",
            Record {
                kind: 17,
                payload: Payload::Structured(m(vec![
                    (0, b(&[1; 16])),
                    (1, b(&[2; 16])),
                    (2, UInt(42)),
                    (3, r(0x11)),
                    (4, b(b"epoch/v1")),
                    (
                        5,
                        Array(vec![
                            Array(vec![b(&[3; 16]), UInt(0)]),
                            Array(vec![b(&[4; 16]), UInt(u64::MAX)]),
                        ]),
                    ),
                    (6, Array(vec![Array(vec![b(&[5; 16]), b(b"offline")])])),
                    (7, r(0x22)),
                    (8, b(b"namespace/v1")),
                ])),
            },
        ),
        (
            "policy-full",
            Record {
                kind: 18,
                payload: Payload::Structured(m(vec![
                    (0, b(&[1; 16])),
                    (1, UInt(2)),
                    (2, Array(vec![b(b"branches/main"), b(b"tags/v1")])),
                    (3, Array(vec![b(b"build"), b(b"test")])),
                    (4, UInt(2)),
                    (5, Array(vec![b(b"key:a")])),
                    (6, UInt(2)),
                    (7, UInt(1)),
                    (8, Array(vec![b(b"acceptance/v1")])),
                    (9, r(0x33)),
                ])),
            },
        ),
    ]);
    vectors.push((
        "view-partitioned-empty",
        Record {
            kind: 6,
            payload: Payload::Structured(m(vec![(0, UInt(0)), (1, Array(vec![])), (5, UInt(1))])),
        },
    ));
    vectors.push((
        "view-partitioned-boundaries",
        Record {
            kind: 6,
            payload: Payload::Structured(m(vec![
                (0, UInt(u64::MAX)),
                (
                    1,
                    Array(vec![
                        Array(vec![UInt(0), r(0x11)]),
                        Array(vec![UInt(255), r(0x22)]),
                    ]),
                ),
                (5, UInt(1)),
            ])),
        },
    ));
    let leaf = Record {
        kind: 6,
        payload: Payload::Structured(m(vec![
            (0, UInt(0)),
            (
                1,
                Array(vec![Array(vec![
                    b(b"branches/main"),
                    UInt(1),
                    Array(vec![UInt(0)]),
                ])]),
            ),
            (2, Array(vec![])),
            (3, Array(vec![])),
            (4, Array(vec![])),
            (5, UInt(0)),
        ])),
    };
    let leaf_bytes = encode(&leaf).unwrap();
    let digest = blake3::hash(&leaf_bytes);
    let mut key = b"KAI-VIEW-BUCKET\0".to_vec();
    key.push(1);
    key.extend(b"branches/main");
    let bucket = blake3::hash(&key).as_bytes()[0];
    vectors.push(("view-submap", leaf));
    vectors.push((
        "view-partitioned-single",
        Record {
            kind: 6,
            payload: Payload::Structured(m(vec![
                (0, UInt(7)),
                (
                    1,
                    Array(vec![Array(vec![
                        UInt(u64::from(bucket)),
                        Array(vec![UInt(1), UInt(1), b(digest.as_bytes())]),
                    ])]),
                ),
                (5, UInt(1)),
            ])),
        },
    ));
    let signing = std::env::args().any(|arg| arg == "--signing-payloads");
    if !signing {
        println!("# name\tframed_hex\tobject_id");
    }
    for (name, record) in vectors {
        let bytes = encode(&record).expect("valid fixture");
        if signing {
            if record.kind == 10 {
                let payload = kai_object_reference::attestation_signing_payload(&bytes).unwrap();
                let hex: String = payload.iter().map(|b| format!("{b:02x}")).collect();
                println!("{name}\t{hex}");
            }
            continue;
        }
        let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        println!("{name}\t{hex}\t{}", id(&bytes).unwrap());
    }
}
