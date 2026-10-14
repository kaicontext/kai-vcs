//! Published schemas are evaluated by an upstream CDDL implementation, not Kai's codecs.
use ciborium::value::Value;
use kai_vcs::Object;
use std::collections::BTreeSet;

const RULES: [&str; 19] = [
    "raw-blob",
    "tree",
    "revision",
    "change-event",
    "conflict",
    "view",
    "operation",
    "checkpoint",
    "evidence",
    "attestation",
    "analysis-manifest",
    "git-envelope",
    "raw-chunk",
    "large-blob",
    "tag",
    "backup-manifest",
    "collaboration-barrier",
    "policy",
    "transfer-manifest",
];
const SCHEMAS: [&str; 7] = [
    include_str!("../docs/schemas.cddl"),
    include_str!("../docs/recovery.cddl"),
    include_str!("../docs/view.cddl"),
    include_str!("../docs/metadata.cddl"),
    include_str!("../docs/git-envelope.cddl"),
    include_str!("../docs/package.cddl"),
    include_str!("../docs/control.cddl"),
];
fn schema(rule: &str) -> String {
    format!("validation-root = {rule}\n{}", SCHEMAS.join("\n"))
}
fn cbor(value: &Value) -> Vec<u8> {
    let mut bytes = Vec::new();
    ciborium::ser::into_writer(value, &mut bytes).unwrap();
    bytes
}
fn fixtures() -> Vec<(&'static str, Vec<u8>)> {
    include_str!("fixtures/objects.tsv")
        .lines()
        .filter(|l| !l.starts_with('#'))
        .map(|line| {
            let mut fields = line.split('\t');
            let name = fields.next().unwrap();
            let hex = fields.next().unwrap();
            let bytes = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect();
            (name, bytes)
        })
        .collect()
}
fn validate(bytes: &[u8]) -> Result<(), String> {
    let kind = u16::from_be_bytes([bytes[13], bytes[14]]);
    let payload = if matches!(kind, 1 | 13) {
        // These two envelopes carry raw bytes; wrap only for the CDDL byte-string rule.
        cbor(&Value::Bytes(bytes[23..].to_vec()))
    } else {
        bytes[23..].to_vec()
    };
    cddl::validate_cbor_from_slice(&schema(RULES[usize::from(kind) - 1]), &payload, None)
        .map_err(|e| e.to_string())
}
fn frame(kind: u16, value: &Value) -> Vec<u8> {
    let payload = cbor(value);
    let mut out = b"KAI-OBJECT\0\0\x01".to_vec();
    out.extend(kind.to_be_bytes());
    out.extend((payload.len() as u64).to_be_bytes());
    out.extend(payload);
    out
}
fn field<'a>(value: &'a mut Value, path: &[u64]) -> &'a mut Value {
    if path.is_empty() {
        return value;
    }
    let next = match value {
        Value::Map(m) => {
            &mut m
                .iter_mut()
                .find(|(k, _)| *k == Value::Integer(path[0].into()))
                .unwrap()
                .1
        }
        Value::Array(a) => &mut a[path[0] as usize],
        _ => panic!("invalid fixture field path"),
    };
    field(next, &path[1..])
}
fn integer(n: i128) -> Value {
    Value::Integer(n.try_into().unwrap())
}
fn assert_agreement(bytes: &[u8], accepted: bool, label: &str) {
    let result = validate(bytes);
    assert_eq!(result.is_ok(), accepted, "CDDL: {label}: {result:?}");
    assert_eq!(Object::decode(bytes).is_ok(), accepted, "kernel: {label}");
    assert_eq!(
        kai_object_reference::decode(bytes).is_ok(),
        accepted,
        "reference: {label}"
    );
}

#[test]
fn published_vectors_cover_all_nineteen_schema_rules() {
    let mut types = BTreeSet::new();
    for (name, bytes) in fixtures() {
        types.insert(u16::from_be_bytes([bytes[13], bytes[14]]));
        assert_agreement(&bytes, true, name);
    }
    assert_eq!(types, (1..=19).collect());
}

#[test]
fn signed_fields_agree_at_and_beyond_i64_boundaries() {
    let fields: &[(&str, &[u64])] = &[
        ("revision-root", &[3, 2]),
        ("revision-root", &[4, 2]),
        ("change-rewrite", &[4, 2]),
        ("checkpoint-full", &[4, 2]),
        ("operation-full", &[5, 2]),
        ("operation-full", &[6]),
        ("operation-full", &[7]),
        ("tag-full", &[2, 2]),
        ("evidence-execution", &[8, 2]),
        ("evidence-execution", &[11]),
        ("evidence-execution", &[12]),
        ("evidence-execution", &[16]),
        ("attestation-full", &[7, 2]),
        ("attestation-full", &[9]),
        ("attestation-full", &[10]),
    ];
    for &(name, path) in fields {
        let original = fixtures().into_iter().find(|(n, _)| *n == name).unwrap().1;
        let kind = u16::from_be_bytes([original[13], original[14]]);
        for (n, accepted) in [
            (i128::from(i64::MIN) - 1, false),
            (i128::from(i64::MIN), true),
            (i128::from(i64::MAX), true),
            (i128::from(i64::MAX) + 1, false),
        ] {
            let mut value: Value = ciborium::de::from_reader(&original[23..]).unwrap();
            // Preserve issuance <= expiry while isolating integer-range validation.
            if name == "attestation-full" && path.len() == 1 {
                *field(&mut value, &[9]) = integer(n);
                *field(&mut value, &[10]) = integer(n);
            } else {
                *field(&mut value, path) = integer(n);
            }
            assert_agreement(
                &frame(kind, &value),
                accepted,
                &format!("{name} {path:?} {n}"),
            );
        }
    }
}

#[test]
fn positive_counters_and_unsigned_generations_agree() {
    for (name, path, positive) in [
        ("view-full", vec![1, 0, 1], true),
        ("policy-full", vec![1], true),
        ("attestation-full", vec![1], true),
        ("view-empty", vec![0], false),
        ("barrier-full", vec![2], false),
    ] {
        let original = fixtures().into_iter().find(|(n, _)| *n == name).unwrap().1;
        let kind = u16::from_be_bytes([original[13], original[14]]);
        for n in [-1, 0, 1, i128::from(u64::MAX)] {
            let mut value: Value = ciborium::de::from_reader(&original[23..]).unwrap();
            *field(&mut value, &path) = integer(n);
            assert_agreement(
                &frame(kind, &value),
                n >= i128::from(positive),
                &format!("{name} {n}"),
            );
        }
    }
}

#[test]
fn structural_checks_do_not_replace_canonical_or_semantic_validation() {
    // Same empty tree, encoded with a forbidden non-minimal array length.
    let mut bytes = b"KAI-OBJECT\0\0\x01\0\x02".to_vec();
    bytes.extend(2u64.to_be_bytes());
    bytes.extend([0x98, 0]);
    assert!(validate(&bytes).is_ok());
    assert!(Object::decode(&bytes).is_err());
    assert!(kai_object_reference::decode(&bytes).is_err());

    // The length fits the schema, but the retained chunk list cannot cover it.
    let original = fixtures()
        .into_iter()
        .find(|(name, _)| *name == "large-manifest")
        .unwrap()
        .1;
    let mut value: Value = ciborium::de::from_reader(&original[23..]).unwrap();
    *field(&mut value, &[0]) = integer(i128::from(u64::MAX));
    let bytes = frame(14, &value);
    assert!(validate(&bytes).is_ok());
    assert!(Object::decode(&bytes).is_err());
    assert!(kai_object_reference::decode(&bytes).is_err());
}

#[test]
fn schemas_reject_missing_extra_and_wrongly_typed_fields() {
    for (name, bytes) in fixtures() {
        let kind = u16::from_be_bytes([bytes[13], bytes[14]]);
        if matches!(kind, 1 | 13) {
            continue;
        }
        let value: Value = ciborium::de::from_reader(&bytes[23..]).unwrap();
        if let Value::Map(entries) = value {
            let mut missing = entries.clone();
            missing.retain(|(key, _)| *key != integer(0));
            assert_agreement(
                &frame(kind, &Value::Map(missing)),
                false,
                &format!("{name} missing key 0"),
            );
            let mut extra = entries.clone();
            extra.push((integer(999), integer(0)));
            assert_agreement(
                &frame(kind, &Value::Map(extra)),
                false,
                &format!("{name} unknown key"),
            );
            let mut wrong = Value::Map(entries);
            *field(&mut wrong, &[0]) = Value::Null;
            assert_agreement(
                &frame(kind, &wrong),
                false,
                &format!("{name} null required field"),
            );
        }
    }
}

#[test]
fn raw_payload_limits_agree_without_changing_wire_framing() {
    for (kind, minimum, maximum) in [(1u16, 0, 16 * 1024 * 1024), (13, 1, 4 * 1024 * 1024)] {
        for size in [0, 1, maximum, maximum + 1] {
            let mut bytes = b"KAI-OBJECT\0\0\x01".to_vec();
            bytes.extend(kind.to_be_bytes());
            bytes.extend((size as u64).to_be_bytes());
            bytes.resize(23 + size, 0);
            assert_agreement(
                &bytes,
                (minimum..=maximum).contains(&size),
                &format!("type {kind} length {size}"),
            );
        }
    }
}

#[test]
fn timezone_bounds_and_large_blob_threshold_agree() {
    for (name, path, cases) in [
        (
            "revision-root",
            vec![3, 3],
            vec![(-1440, false), (-1439, true), (1439, true), (1440, false)],
        ),
        (
            "large-manifest",
            vec![0],
            vec![(16777216, false), (16777217, true)],
        ),
    ] {
        let original = fixtures().into_iter().find(|(n, _)| *n == name).unwrap().1;
        let kind = u16::from_be_bytes([original[13], original[14]]);
        for (n, accepted) in cases {
            let mut value: Value = ciborium::de::from_reader(&original[23..]).unwrap();
            *field(&mut value, &path) = integer(n);
            assert_agreement(&frame(kind, &value), accepted, &format!("{name} {n}"));
        }
    }
}
