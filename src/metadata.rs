//! Experimental advisory metadata registry: docs/METADATA-FORMAT.md.
use crate::{
    Identity, ObjectId,
    codec::{self, Decoder},
    recovery::*,
};

macro_rules! enumeration {
    ($name:ident { $($variant:ident = $value:literal),+ }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        #[repr(u64)]
        pub enum $name { $($variant = $value),+ }
        impl $name {
            fn read(d: &mut Decoder<'_>) -> Result<Self, String> {
                match d.head(0)? { $($value => Ok(Self::$variant),)+ _ => Err(concat!("invalid ", stringify!($name)).into()) }
            }
        }
    }
}
enumeration!(SourceState { Resolved = 1, Conflicted = 2, Unknown = 3 });
enumeration!(EvidenceKind { Execution = 1, Review = 2 });
enumeration!(ClaimedTrust { Unknown = 1, SelfReported = 2, RunnerProduced = 3 });
enumeration!(EvidenceResult { Pass = 1, Fail = 2, Cancelled = 3, TimedOut = 4, Unavailable = 5, Unknown = 6 });
enumeration!(AnalysisCompletion { Complete = 1, Partial = 2, Failed = 3 });

fn strings_valid(values: &[Vec<u8>], sorted: bool) -> Result<(), String> {
    if values.iter().any(Vec::is_empty) || (sorted && !ordered(values)) {
        return Err("empty, duplicate or unordered metadata labels".into());
    }
    Ok(())
}
fn write_strings(out: &mut Vec<u8>, values: &[Vec<u8>]) {
    codec::head(out, 4, values.len() as u64);
    for value in values {
        codec::bytes(out, value);
    }
}
fn read_strings(d: &mut Decoder<'_>) -> Result<Vec<Vec<u8>>, String> {
    let n = d.count(4, 1)?;
    (0..n).map(|_| Ok(d.bytes()?.to_vec())).collect()
}
fn bound<'a>(parts: impl IntoIterator<Item = &'a [u8]>, count: usize) -> Result<(), String> {
    input_bound(
        parts
            .into_iter()
            .fold(count, |n, b| n.saturating_add(b.len()).saturating_add(1)),
    )
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MetadataManifest {
    pub format: Vec<u8>,
    pub object: ObjectId,
}
impl MetadataManifest {
    fn write(&self, out: &mut Vec<u8>) -> Result<(), String> {
        nonempty(&self.format)?;
        codec::head(out, 4, 2);
        codec::bytes(out, &self.format);
        codec::reference(out, self.object);
        Ok(())
    }
    fn read(d: &mut Decoder<'_>) -> Result<Self, String> {
        d.expect(4, 2)?;
        let format = d.bytes()?.to_vec();
        nonempty(&format)?;
        Ok(Self {
            format,
            object: d.reference()?,
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvidenceScope {
    pub covered: Vec<Vec<u8>>,
    pub excluded: Vec<Vec<u8>>,
    pub selection: Vec<u8>,
    pub fallback_reason: Vec<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Evidence {
    pub revision: ObjectId,
    pub tree: ObjectId,
    pub source_state: SourceState,
    pub kind: EvidenceKind,
    pub invocation: Vec<Vec<u8>>,
    pub cwd: Vec<u8>,
    pub inputs: MetadataManifest,
    pub environment: MetadataManifest,
    pub producer: Identity,
    pub claimed_trust: ClaimedTrust,
    pub result: EvidenceResult,
    pub started_seconds: i64,
    pub finished_seconds: i64,
    pub artifacts: Vec<(Vec<u8>, ObjectId)>,
    pub scope: EvidenceScope,
    pub limitations: Vec<Vec<u8>>,
    pub exit_status: Option<i64>,
}
impl Evidence {
    fn validate(&self) -> Result<(), String> {
        if self.invocation.first().is_none_or(Vec::is_empty)
            || (self.kind == EvidenceKind::Review
                && (self.invocation.len() != 1 || self.exit_status.is_some()))
            || (!self.cwd.is_empty() && !path(&self.cwd))
        {
            return Err("invalid evidence invocation, cwd or review exit status".into());
        }
        nonempty(&self.inputs.format)?;
        nonempty(&self.environment.format)?;
        strings_valid(&self.scope.covered, true)?;
        strings_valid(&self.scope.excluded, true)?;
        strings_valid(&self.limitations, false)?;
        if self.artifacts.iter().any(|(name, _)| name.is_empty())
            || !self.artifacts.windows(2).all(|w| w[0].0 < w[1].0)
        {
            return Err("invalid or unordered artifacts".into());
        }
        Ok(())
    }
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        bound(
            self.invocation
                .iter()
                .chain(&self.scope.covered)
                .chain(&self.scope.excluded)
                .chain(&self.limitations)
                .map(Vec::as_slice)
                .chain(self.artifacts.iter().map(|(n, _)| n.as_slice()))
                .chain([
                    self.cwd.as_slice(),
                    &self.inputs.format,
                    &self.environment.format,
                    &self.producer.name,
                    &self.producer.email,
                    &self.scope.selection,
                    &self.scope.fallback_reason,
                ]),
            self.artifacts.len().saturating_mul(37),
        )?;
        self.validate()?;
        let mut out = Vec::new();
        codec::head(&mut out, 5, 16 + u64::from(self.exit_status.is_some()));
        field(&mut out, 0);
        codec::reference(&mut out, self.revision);
        field(&mut out, 1);
        codec::reference(&mut out, self.tree);
        field(&mut out, 2);
        field(&mut out, self.source_state as u64);
        field(&mut out, 3);
        field(&mut out, self.kind as u64);
        field(&mut out, 4);
        write_strings(&mut out, &self.invocation);
        field(&mut out, 5);
        codec::bytes(&mut out, &self.cwd);
        field(&mut out, 6);
        self.inputs.write(&mut out)?;
        field(&mut out, 7);
        self.environment.write(&mut out)?;
        field(&mut out, 8);
        self.producer.encode(&mut out)?;
        field(&mut out, 9);
        field(&mut out, self.claimed_trust as u64);
        field(&mut out, 10);
        field(&mut out, self.result as u64);
        field(&mut out, 11);
        codec::signed(&mut out, self.started_seconds);
        field(&mut out, 12);
        codec::signed(&mut out, self.finished_seconds);
        field(&mut out, 13);
        write_manifest(&mut out, &self.artifacts);
        field(&mut out, 14);
        codec::head(&mut out, 5, 4);
        field(&mut out, 0);
        write_strings(&mut out, &self.scope.covered);
        field(&mut out, 1);
        write_strings(&mut out, &self.scope.excluded);
        field(&mut out, 2);
        codec::bytes(&mut out, &self.scope.selection);
        field(&mut out, 3);
        codec::bytes(&mut out, &self.scope.fallback_reason);
        field(&mut out, 15);
        write_strings(&mut out, &self.limitations);
        if let Some(exit) = self.exit_status {
            field(&mut out, 16);
            codec::signed(&mut out, exit);
        }
        codec::bounded(out, 9)
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (mut d, count) = decoder(input, 9, 16, 17)?;
        d.expect(0, 0)?;
        let revision = d.reference()?;
        d.expect(0, 1)?;
        let tree = d.reference()?;
        d.expect(0, 2)?;
        let source_state = SourceState::read(&mut d)?;
        d.expect(0, 3)?;
        let kind = EvidenceKind::read(&mut d)?;
        d.expect(0, 4)?;
        let invocation = read_strings(&mut d)?;
        d.expect(0, 5)?;
        let cwd = d.bytes()?.to_vec();
        d.expect(0, 6)?;
        let inputs = MetadataManifest::read(&mut d)?;
        d.expect(0, 7)?;
        let environment = MetadataManifest::read(&mut d)?;
        d.expect(0, 8)?;
        let producer = Identity::decode(&mut d)?;
        d.expect(0, 9)?;
        let claimed_trust = ClaimedTrust::read(&mut d)?;
        d.expect(0, 10)?;
        let result = EvidenceResult::read(&mut d)?;
        d.expect(0, 11)?;
        let started_seconds = d.signed()?;
        d.expect(0, 12)?;
        let finished_seconds = d.signed()?;
        d.expect(0, 13)?;
        let n = d.count(4, 39)?;
        let mut artifacts = Vec::new();
        for _ in 0..n {
            d.expect(4, 2)?;
            artifacts.push((d.bytes()?.to_vec(), d.reference()?));
        }
        d.expect(0, 14)?;
        d.expect(5, 4)?;
        d.expect(0, 0)?;
        let covered = read_strings(&mut d)?;
        d.expect(0, 1)?;
        let excluded = read_strings(&mut d)?;
        d.expect(0, 2)?;
        let selection = d.bytes()?.to_vec();
        d.expect(0, 3)?;
        let fallback_reason = d.bytes()?.to_vec();
        d.expect(0, 15)?;
        let limitations = read_strings(&mut d)?;
        let exit_status = if count == 17 {
            d.expect(0, 16)?;
            Some(d.signed()?)
        } else {
            None
        };
        d.finish()?;
        let value = Self {
            revision,
            tree,
            source_state,
            kind,
            invocation,
            cwd,
            inputs,
            environment,
            producer,
            claimed_trust,
            result,
            started_seconds,
            finished_seconds,
            artifacts,
            scope: EvidenceScope {
                covered,
                excluded,
                selection,
                fallback_reason,
            },
            limitations,
            exit_status,
        };
        value.validate()?;
        Ok(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Attestation {
    pub format: Vec<u8>,
    pub version: u64,
    pub domain: Vec<u8>,
    pub subject_type: u16,
    pub subject: ObjectId,
    pub purpose: Vec<u8>,
    pub signer: Vec<u8>,
    pub claimed_identity: Identity,
    pub signature: Vec<u8>,
    pub issued_seconds: Option<i64>,
    pub expires_seconds: Option<i64>,
}
impl Attestation {
    fn fields(&self, include_signature: bool) -> Result<Vec<u8>, String> {
        bound(
            [
                self.format.as_slice(),
                &self.domain,
                &self.purpose,
                &self.signer,
                &self.claimed_identity.name,
                &self.claimed_identity.email,
                &self.signature,
            ],
            0,
        )?;
        for b in [&self.format, &self.domain, &self.purpose, &self.signer] {
            nonempty(b)?;
        }
        if include_signature {
            nonempty(&self.signature)?;
        }
        if self.version == 0
            || !(1..=19).contains(&self.subject_type)
            || matches!((self.issued_seconds,self.expires_seconds),(Some(a),Some(b)) if b<a)
        {
            return Err("invalid attestation version, subject or time interval".into());
        }
        let mut out = Vec::new();
        codec::head(
            &mut out,
            5,
            8 + u64::from(include_signature)
                + u64::from(self.issued_seconds.is_some())
                + u64::from(self.expires_seconds.is_some()),
        );
        field(&mut out, 0);
        codec::bytes(&mut out, &self.format);
        field(&mut out, 1);
        field(&mut out, self.version);
        field(&mut out, 2);
        codec::bytes(&mut out, &self.domain);
        field(&mut out, 3);
        field(&mut out, self.subject_type as u64);
        field(&mut out, 4);
        codec::reference(&mut out, self.subject);
        field(&mut out, 5);
        codec::bytes(&mut out, &self.purpose);
        field(&mut out, 6);
        codec::bytes(&mut out, &self.signer);
        field(&mut out, 7);
        self.claimed_identity.encode(&mut out)?;
        if include_signature {
            field(&mut out, 8);
            codec::bytes(&mut out, &self.signature);
        }
        if let Some(n) = self.issued_seconds {
            field(&mut out, 9);
            codec::signed(&mut out, n);
        }
        if let Some(n) = self.expires_seconds {
            field(&mut out, 10);
            codec::signed(&mut out, n);
        }
        input_bound(out.len())?;
        Ok(out)
    }
    pub fn signing_payload(&self) -> Result<Vec<u8>, String> {
        let mut out = b"KAI-ATTESTATION\0".to_vec();
        out.extend(self.fields(false)?);
        Ok(out)
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        codec::bounded(self.fields(true)?, 10)
    }
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (mut d, count) = decoder(input, 10, 9, 11)?;
        d.expect(0, 0)?;
        let format = d.bytes()?.to_vec();
        d.expect(0, 1)?;
        let version = d.head(0)?;
        d.expect(0, 2)?;
        let domain = d.bytes()?.to_vec();
        d.expect(0, 3)?;
        let subject_type = u16::try_from(d.head(0)?).map_err(|_| "subject type overflow")?;
        d.expect(0, 4)?;
        let subject = d.reference()?;
        d.expect(0, 5)?;
        let purpose = d.bytes()?.to_vec();
        d.expect(0, 6)?;
        let signer = d.bytes()?.to_vec();
        d.expect(0, 7)?;
        let claimed_identity = Identity::decode(&mut d)?;
        d.expect(0, 8)?;
        let signature = d.bytes()?.to_vec();
        let mut issued_seconds = None;
        let mut expires_seconds = None;
        let mut previous = 8;
        for _ in 9..count {
            match next_key(&mut d, &mut previous)? {
                9 => issued_seconds = Some(d.signed()?),
                10 => expires_seconds = Some(d.signed()?),
                _ => return Err("unknown attestation field".into()),
            }
        }
        d.finish()?;
        let value = Self {
            format,
            version,
            domain,
            subject_type,
            subject,
            purpose,
            signer,
            claimed_identity,
            signature,
            issued_seconds,
            expires_seconds,
        };
        value.fields(true)?;
        Ok(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisManifest {
    pub tree: ObjectId,
    pub analyzer: Vec<u8>,
    pub resolver: Vec<u8>,
    pub dependency_digest: [u8; 32],
    pub build_digest: [u8; 32],
    pub platform: Vec<u8>,
    pub completion: AnalysisCompletion,
    pub covered: Vec<Vec<u8>>,
    pub unsupported: Vec<Vec<u8>>,
    pub parser_failures: Vec<Vec<u8>>,
    pub unresolved: Vec<Vec<u8>>,
    pub results: Vec<ObjectId>,
    pub reason: Vec<u8>,
}
impl AnalysisManifest {
    fn validate(&self) -> Result<(), String> {
        for b in [&self.analyzer, &self.resolver, &self.platform, &self.reason] {
            nonempty(b)?;
        }
        for labels in [
            &self.covered,
            &self.unsupported,
            &self.parser_failures,
            &self.unresolved,
        ] {
            strings_valid(labels, true)?;
        }
        if !ordered(&self.results)
            || (self.completion == AnalysisCompletion::Complete
                && (!self.unsupported.is_empty()
                    || !self.parser_failures.is_empty()
                    || !self.unresolved.is_empty()))
        {
            return Err("unordered results or contradictory complete analysis".into());
        }
        Ok(())
    }
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        bound(
            self.covered
                .iter()
                .chain(&self.unsupported)
                .chain(&self.parser_failures)
                .chain(&self.unresolved)
                .map(Vec::as_slice)
                .chain([
                    self.analyzer.as_slice(),
                    &self.resolver,
                    &self.platform,
                    &self.reason,
                ]),
            self.results.len().saturating_mul(37),
        )?;
        self.validate()?;
        let mut out = Vec::new();
        codec::head(&mut out, 5, 13);
        field(&mut out, 0);
        codec::reference(&mut out, self.tree);
        field(&mut out, 1);
        codec::bytes(&mut out, &self.analyzer);
        field(&mut out, 2);
        codec::bytes(&mut out, &self.resolver);
        for (key, digest) in [(3, &self.dependency_digest), (4, &self.build_digest)] {
            field(&mut out, key);
            codec::head(&mut out, 4, 2);
            field(&mut out, 1);
            codec::bytes(&mut out, digest);
        }
        field(&mut out, 5);
        codec::bytes(&mut out, &self.platform);
        field(&mut out, 6);
        field(&mut out, self.completion as u64);
        for (key, labels) in [
            (7, &self.covered),
            (8, &self.unsupported),
            (9, &self.parser_failures),
            (10, &self.unresolved),
        ] {
            field(&mut out, key);
            write_strings(&mut out, labels);
        }
        field(&mut out, 11);
        write_refs(&mut out, &self.results);
        field(&mut out, 12);
        codec::bytes(&mut out, &self.reason);
        codec::bounded(out, 11)
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (mut d, _) = decoder(input, 11, 13, 13)?;
        d.expect(0, 0)?;
        let tree = d.reference()?;
        d.expect(0, 1)?;
        let analyzer = d.bytes()?.to_vec();
        d.expect(0, 2)?;
        let resolver = d.bytes()?.to_vec();
        d.expect(0, 3)?;
        d.expect(4, 2)?;
        d.expect(0, 1)?;
        let dependency_digest = fixed(&mut d)?;
        d.expect(0, 4)?;
        d.expect(4, 2)?;
        d.expect(0, 1)?;
        let build_digest = fixed(&mut d)?;
        d.expect(0, 5)?;
        let platform = d.bytes()?.to_vec();
        d.expect(0, 6)?;
        let completion = AnalysisCompletion::read(&mut d)?;
        d.expect(0, 7)?;
        let covered = read_strings(&mut d)?;
        d.expect(0, 8)?;
        let unsupported = read_strings(&mut d)?;
        d.expect(0, 9)?;
        let parser_failures = read_strings(&mut d)?;
        d.expect(0, 10)?;
        let unresolved = read_strings(&mut d)?;
        d.expect(0, 11)?;
        let results = read_refs(&mut d)?;
        d.expect(0, 12)?;
        let reason = d.bytes()?.to_vec();
        d.finish()?;
        let value = Self {
            tree,
            analyzer,
            resolver,
            dependency_digest,
            build_digest,
            platform,
            completion,
            covered,
            unsupported,
            parser_failures,
            unresolved,
            results,
            reason,
        };
        value.validate()?;
        Ok(value)
    }
}
