//! Read-only package contracts; no persistence or publication authority.
use crate::{
    ClosureLimits, Object, ObjectId,
    codec::{self, Decoder},
    recovery::*,
};
use std::collections::BTreeMap;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum PackageCompleteness {
    Standalone = 1,
    Incremental = 2,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u64)]
pub enum Interpretation {
    Interpreted = 1,
    Opaque = 2,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageRoot {
    pub id: ObjectId,
    pub type_code: u16,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InventoryEntry {
    pub id: ObjectId,
    pub type_code: u16,
    pub framed_length: u64,
    pub interpretation: Interpretation,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackageContents {
    pub completeness: PackageCompleteness,
    pub roots: Vec<PackageRoot>,
    pub inventory: Vec<InventoryEntry>,
    pub prerequisites: Vec<InventoryEntry>,
    pub required_features: Vec<Vec<u8>>,
    pub optional_features: Vec<Vec<u8>>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferManifest {
    pub repository: [u8; 16],
    pub contents: PackageContents,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackupManifest {
    pub repository: [u8; 16],
    pub generation: u64,
    pub current_view: ObjectId,
    pub current_operation: Option<ObjectId>,
    pub contents: PackageContents,
}
impl PackageContents {
    fn validate(&self) -> Result<(), String> {
        if !self.roots.windows(2).all(|w| w[0].id < w[1].id) {
            return Err("unordered package roots".into());
        }
        let mut entries = BTreeMap::new();
        for list in [&self.inventory, &self.prerequisites] {
            if !list.windows(2).all(|w| w[0].id < w[1].id) {
                return Err("unordered inventory".into());
            }
            for e in list {
                if e.type_code == 0
                    || !(23..=codec::STRUCTURED_LIMIT as u64 + 23).contains(&e.framed_length)
                    || entries.insert(e.id, e).is_some()
                {
                    return Err("invalid or overlapping inventory".into());
                }
            }
        }
        if (self.completeness == PackageCompleteness::Standalone) != self.prerequisites.is_empty() {
            return Err("completeness contradicts prerequisites".into());
        }
        for root in &self.roots {
            self.check_root(root.id, root.type_code)?;
        }
        for features in [&self.required_features, &self.optional_features] {
            if features.iter().any(Vec::is_empty) || !ordered(features) {
                return Err("invalid feature list".into());
            }
        }
        if self
            .required_features
            .iter()
            .any(|f| self.optional_features.binary_search(f).is_ok())
        {
            return Err("overlapping feature lists".into());
        }
        Ok(())
    }
    fn check_root(&self, id: ObjectId, kind: u16) -> Result<(), String> {
        if !(1..=19).contains(&kind) {
            return Err("invalid root type".into());
        }
        let e = self
            .inventory
            .binary_search_by_key(&id, |e| e.id)
            .ok()
            .map(|i| &self.inventory[i])
            .or_else(|| {
                self.prerequisites
                    .binary_search_by_key(&id, |e| e.id)
                    .ok()
                    .map(|i| &self.prerequisites[i])
            });
        if !e
            .is_some_and(|e| e.type_code == kind && e.interpretation == Interpretation::Interpreted)
        {
            return Err("root absent or mistyped in inventory".into());
        }
        Ok(())
    }
    fn write(&self, out: &mut Vec<u8>) -> Result<(), String> {
        let count = self
            .inventory
            .len()
            .saturating_add(self.prerequisites.len())
            .saturating_add(self.roots.len());
        let size = self
            .required_features
            .iter()
            .chain(&self.optional_features)
            .fold(count.saturating_mul(39), |n, b| {
                n.saturating_add(b.len()).saturating_add(1)
            });
        input_bound(size)?;
        self.validate()?;
        codec::head(out, 5, 7);
        field(out, 0);
        field(out, self.completeness as u64);
        field(out, 1);
        codec::head(out, 4, self.roots.len() as u64);
        for r in &self.roots {
            codec::head(out, 4, 2);
            codec::reference(out, r.id);
            field(out, r.type_code as u64);
        }
        for (key, list) in [(2, &self.inventory), (3, &self.prerequisites)] {
            field(out, key);
            codec::head(out, 4, list.len() as u64);
            for e in list {
                codec::head(out, 4, 4);
                codec::reference(out, e.id);
                field(out, e.type_code as u64);
                field(out, e.framed_length);
                field(out, e.interpretation as u64);
            }
        }
        for (key, list) in [(4, &self.required_features), (5, &self.optional_features)] {
            field(out, key);
            codec::head(out, 4, list.len() as u64);
            for f in list {
                codec::bytes(out, f);
            }
        }
        field(out, 6);
        field(out, 1);
        Ok(())
    }
    fn read(d: &mut Decoder<'_>) -> Result<Self, String> {
        d.expect(5, 7)?;
        d.expect(0, 0)?;
        let completeness = match d.head(0)? {
            1 => PackageCompleteness::Standalone,
            2 => PackageCompleteness::Incremental,
            _ => return Err("invalid completeness".into()),
        };
        d.expect(0, 1)?;
        let n = d.count(4, 39)?;
        let mut roots = Vec::new();
        for _ in 0..n {
            d.expect(4, 2)?;
            roots.push(PackageRoot {
                id: d.reference()?,
                type_code: kind(d)?,
            });
        }
        let mut lists = Vec::new();
        for key in [2, 3] {
            d.expect(0, key)?;
            let n = d.count(4, 41)?;
            let mut list = Vec::new();
            for _ in 0..n {
                d.expect(4, 4)?;
                list.push(InventoryEntry {
                    id: d.reference()?,
                    type_code: kind(d)?,
                    framed_length: d.head(0)?,
                    interpretation: match d.head(0)? {
                        1 => Interpretation::Interpreted,
                        2 => Interpretation::Opaque,
                        _ => return Err("invalid interpretation".into()),
                    },
                });
            }
            lists.push(list);
        }
        let mut features = Vec::new();
        for key in [4, 5] {
            d.expect(0, key)?;
            let n = d.count(4, 2)?;
            let mut list = Vec::new();
            for _ in 0..n {
                list.push(d.bytes()?.to_vec());
            }
            features.push(list);
        }
        d.expect(0, 6)?;
        d.expect(0, 1)?;
        let optional_features = features.pop().unwrap();
        let required_features = features.pop().unwrap();
        let prerequisites = lists.pop().unwrap();
        let inventory = lists.pop().unwrap();
        let v = Self {
            completeness,
            roots,
            inventory,
            prerequisites,
            required_features,
            optional_features,
        };
        v.validate()?;
        Ok(v)
    }
}
fn kind(d: &mut Decoder<'_>) -> Result<u16, String> {
    u16::try_from(d.head(0)?).map_err(|_| "type overflow".into())
}
impl TransferManifest {
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        if self.contents.roots.is_empty() {
            return Err("transfer requires selected roots".into());
        }
        let mut out = Vec::new();
        codec::head(&mut out, 5, 2);
        field(&mut out, 0);
        codec::bytes(&mut out, &self.repository);
        field(&mut out, 1);
        self.contents.write(&mut out)?;
        codec::bounded(out, 19)
    }
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (mut d, _) = decoder(input, 19, 2, 2)?;
        d.expect(0, 0)?;
        let repository = fixed(&mut d)?;
        d.expect(0, 1)?;
        let contents = PackageContents::read(&mut d)?;
        d.finish()?;
        if contents.roots.is_empty() {
            return Err("transfer requires selected roots".into());
        }
        Ok(Self {
            repository,
            contents,
        })
    }
}
impl BackupManifest {
    fn validate(&self) -> Result<(), String> {
        self.contents.validate()?;
        self.contents.check_root(self.current_view, 6)?;
        if let Some(op) = self.current_operation {
            self.contents.check_root(op, 7)?;
        } else if self.generation != 0 {
            return Err("noninitial backup requires current operation".into());
        }
        Ok(())
    }
    pub fn encode(&self) -> Result<Vec<u8>, String> {
        self.validate()?;
        let mut out = Vec::new();
        codec::head(&mut out, 5, 4 + u64::from(self.current_operation.is_some()));
        field(&mut out, 0);
        codec::bytes(&mut out, &self.repository);
        field(&mut out, 1);
        field(&mut out, self.generation);
        field(&mut out, 2);
        codec::reference(&mut out, self.current_view);
        if let Some(op) = self.current_operation {
            field(&mut out, 3);
            codec::reference(&mut out, op);
        }
        field(&mut out, 4);
        self.contents.write(&mut out)?;
        codec::bounded(out, 16)
    }
    pub fn id(&self) -> Result<ObjectId, String> {
        Ok(ObjectId::hash(&self.encode()?))
    }
    pub fn decode(input: &[u8]) -> Result<Self, String> {
        let (mut d, n) = decoder(input, 16, 4, 5)?;
        d.expect(0, 0)?;
        let repository = fixed(&mut d)?;
        d.expect(0, 1)?;
        let generation = d.head(0)?;
        d.expect(0, 2)?;
        let current_view = d.reference()?;
        let current_operation = if n == 5 {
            d.expect(0, 3)?;
            Some(d.reference()?)
        } else {
            None
        };
        d.expect(0, 4)?;
        let contents = PackageContents::read(&mut d)?;
        d.finish()?;
        let v = Self {
            repository,
            generation,
            current_view,
            current_operation,
            contents,
        };
        v.validate()?;
        Ok(v)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PackageLocation {
    Packaged,
    Prerequisite,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerifiedPackage {
    StandaloneVerified,
    IncrementalVerified,
}
/// Validate bytes and closure only; caller supplies immutable bounded reads under a lease.
pub fn verify_package(
    manifest: &Object,
    supported_features: &[Vec<u8>],
    limits: ClosureLimits,
    mut read: impl FnMut(ObjectId, PackageLocation) -> Result<Vec<u8>, String>,
) -> Result<VerifiedPackage, String> {
    let (contents, backup) = match manifest {
        Object::TransferManifest(v) => {
            v.encode()?;
            (&v.contents, None)
        }
        Object::BackupManifest(v) => {
            v.encode()?;
            (&v.contents, Some(v.as_ref()))
        }
        _ => return Err("expected backup or transfer manifest".into()),
    };
    if contents
        .required_features
        .iter()
        .any(|f| !supported_features.contains(f))
    {
        return Err("unsupported required package feature".into());
    }
    if contents
        .inventory
        .len()
        .saturating_add(contents.prerequisites.len())
        > limits.max_objects
    {
        return Err("package object limit exceeded".into());
    }
    let mut declared = BTreeMap::new();
    let mut roots = Vec::new();
    let mut bytes = 0u64;
    let mut current_post_view = None;
    for (location, list) in [
        (PackageLocation::Packaged, &contents.inventory),
        (PackageLocation::Prerequisite, &contents.prerequisites),
    ] {
        for e in list {
            bytes = bytes
                .checked_add(e.framed_length)
                .ok_or("package byte overflow")?;
            if bytes > limits.max_bytes {
                return Err("package byte limit exceeded".into());
            }
            let raw = read(e.id, location)?;
            let (kind, _) = codec::unframe(&raw)?;
            if kind != e.type_code
                || raw.len() as u64 != e.framed_length
                || ObjectId::hash(&raw) != e.id
            {
                return Err("package inventory hash, type or length mismatch".into());
            }
            if e.interpretation == Interpretation::Interpreted {
                let obj = Object::decode(&raw)?;
                if backup.is_some_and(|b| b.current_operation == Some(e.id)) {
                    let Object::Operation(op) = obj else {
                        return Err("backup operation has wrong type".into());
                    };
                    current_post_view = Some(op.post_view);
                }
                roots.push(e.id);
            }
            declared.insert(e.id, (e, location));
        }
    }
    if let Some(b) = backup
        && b.current_operation.is_some()
        && current_post_view != Some(b.current_view)
    {
        return Err("backup operation/view linkage mismatch".into());
    }
    crate::verify_closure(
        &roots,
        ClosureLimits {
            max_bytes: limits.max_bytes - bytes,
            ..limits
        },
        |id| {
            let (e, location) = declared
                .get(&id)
                .ok_or("required object absent from package inventory")?;
            if e.interpretation != Interpretation::Interpreted {
                return Err("opaque metadata cannot satisfy required closure".into());
            }
            read(id, *location)
        },
    )?;
    Ok(match contents.completeness {
        PackageCompleteness::Standalone => VerifiedPackage::StandaloneVerified,
        PackageCompleteness::Incremental => VerifiedPackage::IncrementalVerified,
    })
}
