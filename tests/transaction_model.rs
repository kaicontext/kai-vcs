//! P0 oracle, intentionally independent of production codecs/storage.
//! This models allowed outcomes, not filesystem or power-loss behavior.
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq)]
struct Ref {
    generation: u64,
    target: Option<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Update {
    name: &'static str,
    expected: Ref,
    target: Option<u8>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Request {
    id: u64,
    generation: u64,
    updates: Vec<Update>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Receipt {
    generation: u64,
}
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct Model {
    generation: u64,
    objects: BTreeMap<u8, Vec<u8>>,
    refs: BTreeMap<&'static str, Ref>,
    receipts: BTreeMap<u64, (Request, Receipt)>,
    expired: BTreeSet<u64>,
    recovery: Vec<BTreeMap<&'static str, Ref>>,
    leases: BTreeSet<u8>,
}

impl Model {
    fn get_ref(&self, name: &'static str) -> Ref {
        self.refs.get(name).cloned().unwrap_or(Ref {
            generation: 0,
            target: None,
        })
    }
    fn request(&self, id: u64, name: &'static str, target: Option<u8>) -> Request {
        Request {
            id,
            generation: self.generation,
            updates: vec![Update {
                name,
                expected: self.get_ref(name),
                target,
            }],
        }
    }
    fn closure(&self, roots: impl IntoIterator<Item = u8>) -> Result<BTreeSet<u8>, &'static str> {
        // Small independent DFS also rejects cycles, unlike a visited-only walk.
        fn visit(
            model: &Model,
            id: u8,
            active: &mut BTreeSet<u8>,
            done: &mut BTreeSet<u8>,
        ) -> Result<(), &'static str> {
            if active.contains(&id) {
                return Err("cycle");
            }
            if done.contains(&id) {
                return Ok(());
            }
            active.insert(id);
            for next in model.objects.get(&id).ok_or("missing closure")? {
                visit(model, *next, active, done)?;
            }
            active.remove(&id);
            done.insert(id);
            Ok(())
        }
        let mut done = BTreeSet::new();
        let mut active = BTreeSet::new();
        for root in roots {
            visit(self, root, &mut active, &mut done)?;
        }
        Ok(done)
    }
    fn install(&mut self, id: u8, children: Vec<u8>) -> Result<(), &'static str> {
        if let Some(old) = self.objects.get(&id) {
            if old != &children {
                return Err("immutable collision");
            }
        } else {
            self.objects.insert(id, children);
        }
        Ok(())
    }
    fn commit(&mut self, request: Request) -> Result<Receipt, &'static str> {
        if let Some((old, receipt)) = self.receipts.get(&request.id) {
            return if old == &request {
                Ok(receipt.clone())
            } else {
                Err("idempotency mismatch")
            };
        }
        if self.expired.contains(&request.id) {
            return Err("unknown expired outcome");
        }
        if self.generation != request.generation {
            return Err("stale repository generation");
        }
        let mut names = BTreeSet::new();
        for update in &request.updates {
            if !names.insert(update.name) {
                return Err("duplicate update");
            }
            if self.get_ref(update.name) != update.expected {
                return Err("stale ref generation/target");
            }
        }
        self.closure(request.updates.iter().filter_map(|u| u.target))?;
        // One atomic root transaction; preparation/objects alone never reach here.
        let before = self.refs.clone();
        for update in &request.updates {
            self.refs.insert(
                update.name,
                Ref {
                    generation: update.expected.generation + 1,
                    target: update.target,
                },
            );
        }
        self.recovery.push(before);
        self.generation += 1;
        let receipt = Receipt {
            generation: self.generation,
        };
        self.receipts.insert(request.id, (request, receipt.clone()));
        Ok(receipt)
    }
    fn live_roots(&self) -> BTreeSet<u8> {
        self.refs
            .values()
            .filter_map(|r| r.target)
            .chain(
                self.recovery
                    .iter()
                    .flat_map(|v| v.values().filter_map(|r| r.target)),
            )
            .chain(self.leases.iter().copied())
            .collect()
    }
    fn gc(&mut self) -> Result<(), &'static str> {
        let keep = self.closure(self.live_roots())?;
        self.objects.retain(|id, _| keep.contains(id));
        Ok(())
    }
    fn undo(&mut self, id: u64) -> Result<Receipt, &'static str> {
        let old = self.recovery.last().ok_or("recovery expired")?;
        let names: BTreeSet<_> = self.refs.keys().chain(old.keys()).copied().collect();
        let updates = names
            .into_iter()
            .map(|name| Update {
                name,
                expected: self.get_ref(name),
                target: old.get(name).and_then(|r| r.target),
            })
            .collect();
        self.commit(Request {
            id,
            generation: self.generation,
            updates,
        })
    }
    fn invariant(&self) {
        self.closure(self.live_roots())
            .expect("every retained root has full closure");
    }
}

fn populated() -> Model {
    let mut m = Model::default();
    m.install(1, vec![]).unwrap();
    m.install(2, vec![1]).unwrap();
    m.install(3, vec![2]).unwrap();
    m
}

#[test]
fn oracle_t017_one_of_100_same_generation_writers_commits() {
    let mut m = populated();
    let requests: Vec<_> = (0..100).map(|i| m.request(i, "main", Some(3))).collect();
    assert_eq!(
        requests
            .into_iter()
            .filter(|r| m.commit(r.clone()).is_ok())
            .count(),
        1
    );
    m.invariant();
}

#[test]
fn oracle_t018_aba_and_deleted_ref_generations_reject_stale_requests() {
    let mut m = populated();
    m.commit(m.request(1, "main", Some(1))).unwrap();
    let stale = m.request(2, "main", Some(3));
    m.commit(m.request(3, "main", Some(2))).unwrap();
    m.commit(m.request(4, "main", Some(1))).unwrap();
    let before = m.clone();
    assert!(m.commit(stale).is_err());
    assert_eq!(m, before);
    let stale = m.get_ref("main");
    m.commit(m.request(5, "main", None)).unwrap();
    m.commit(m.request(6, "main", Some(1))).unwrap();
    let mut request = m.request(7, "main", Some(3));
    request.updates[0].expected = stale;
    let before = m.clone();
    assert!(m.commit(request).is_err());
    assert_eq!(m, before);
}

#[test]
fn oracle_t019_batch_rejection_never_applies_a_prefix() {
    let mut m = populated();
    let mut request = m.request(1, "main", Some(1));
    request.updates.push(Update {
        name: "release",
        expected: Ref {
            generation: 99,
            target: None,
        },
        target: Some(2),
    });
    let before = m.clone();
    assert!(m.commit(request).is_err());
    assert_eq!(m, before);
}

#[test]
fn oracle_t022_lost_response_retries_exactly_once_and_t023_expiry_refuses() {
    let mut m = populated();
    let request = m.request(1, "main", Some(3));
    let receipt = m.commit(request.clone()).unwrap();
    let before = m.clone();
    assert_eq!(m.commit(request.clone()).unwrap(), receipt);
    assert_eq!(m, before);
    let mut different = request.clone();
    different.updates[0].target = Some(1);
    assert!(m.commit(different).is_err());
    assert_eq!(m, before);
    m.receipts.remove(&1);
    m.expired.insert(1);
    let before = m.clone();
    assert_eq!(m.commit(request), Err("unknown expired outcome"));
    assert_eq!(m, before);
}

#[test]
fn oracle_missing_closure_and_prepared_candidates_never_become_history() {
    let mut m = populated();
    m.install(4, vec![99]).unwrap();
    let before = m.clone();
    assert!(m.commit(m.request(1, "main", Some(4))).is_err());
    assert_eq!(m, before);
    // A simulated restart preserves committed roots; it does not elect a prepared object.
    let restarted = m.clone();
    assert!(restarted.refs.is_empty());
    assert_eq!(restarted.generation, 0);
    m.install(5, vec![5]).unwrap();
    let before = m.clone();
    assert!(m.commit(m.request(2, "main", Some(5))).is_err());
    assert_eq!(m, before);
}

#[test]
fn oracle_gc_preserves_ancestry_leases_and_undo_until_explicit_expiry() {
    let mut m = populated();
    m.commit(m.request(1, "main", Some(3))).unwrap();
    m.commit(m.request(2, "main", Some(1))).unwrap();
    m.gc().unwrap();
    assert!(m.objects.contains_key(&3));
    m.undo(3).unwrap();
    assert_eq!(m.get_ref("main").target, Some(3));
    m.recovery.clear();
    m.gc().unwrap();
    assert_eq!(m.objects.len(), 3); // source parents never expire
    m.commit(m.request(4, "main", Some(1))).unwrap();
    m.recovery.clear();
    m.leases.insert(3);
    m.gc().unwrap();
    assert_eq!(m.objects.len(), 3);
    m.leases.clear();
    m.gc().unwrap();
    assert_eq!(m.objects.len(), 1);
    assert_eq!(m.undo(5), Err("recovery expired"));
}

#[test]
fn oracle_explores_save_undo_gc_lease_crash_sequences() {
    fn explore(model: Model, depth: usize, visited: &mut usize) {
        *visited += 1;
        model.invariant();
        if depth == 0 {
            return;
        }
        for action in 0..7 {
            let mut next = model.clone();
            match action {
                0 | 1 => {
                    // Install prepared bytes first; branch ancestry remains canonical.
                    next.install(1, vec![]).unwrap();
                    next.install(2, vec![1]).unwrap();
                    let request = next.request(next.generation + 1, "main", Some(action + 1));
                    let _ = next.commit(request);
                }
                2 => {
                    let _ = next.undo(next.generation + 1);
                }
                3 => {
                    next.gc().unwrap();
                }
                4 => {
                    next.recovery.clear();
                }
                5 => {
                    if next.objects.contains_key(&2) {
                        next.leases.insert(2);
                    }
                }
                6 => {
                    next.leases.clear();
                } // crashed preparer releases its lease
                _ => unreachable!(),
            }
            explore(next, depth - 1, visited);
        }
    }
    let mut visited = 0;
    explore(Model::default(), 6, &mut visited);
    assert_eq!(visited, 137257);
}
