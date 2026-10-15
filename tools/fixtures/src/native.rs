use super::*;
pub(super) fn generate(g: &mut Generator) -> Result<()> {
    let mut random = vec![0; 4096];
    let mut state = SEED;
    for byte in &mut random {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        *byte = state as u8;
    }
    let f01 = vec![
        ("empty", vec![]),
        ("binary", random),
        ("invalid-utf8", vec![255, 254, 192, 128]),
        ("crlf", b"one\r\ntwo\r\n".to_vec()),
        ("lf", b"one\ntwo\n".to_vec()),
        ("mixed", b"one\r\ntwo\nthree\r".to_vec()),
        ("trailing", b"tabs\t \nspaces   \n".to_vec()),
        ("bom", b"\xef\xbb\xbftext\n".to_vec()),
        ("huge-line", vec![b'x'; 1024 * 1024]),
        ("nul", b"before\0after\0".to_vec()),
        (
            "markers",
            b"<<<<<<< ours\n=======\n>>>>>>> theirs\n".to_vec(),
        ),
    ];
    g.snapshot(
        "F01",
        "byte-zoo",
        f01.into_iter()
            .map(|(n, b)| (n.as_bytes().to_vec(), EntryKind::Regular, b))
            .collect(),
    )?;
    g.snapshot(
        "F02",
        "kinds",
        vec![
            (
                b"exec".to_vec(),
                EntryKind::Executable,
                b"#!/bin/sh\nexit 0\n".to_vec(),
            ),
            (b"relative".to_vec(), EntryKind::Symlink, b"exec".to_vec()),
            (
                b"dangling".to_vec(),
                EntryKind::Symlink,
                b"missing".to_vec(),
            ),
            (
                b"absolute".to_vec(),
                EntryKind::Symlink,
                b"/outside/fixture".to_vec(),
            ),
            (b"loop".to_vec(), EntryKind::Symlink, b"loop".to_vec()),
        ],
    )?;
    for (name, paths) in [
        ("raw-name", vec![vec![255]]),
        ("case-collision", vec![b"A".to_vec(), b"a".to_vec()]),
        (
            "unicode-collision",
            vec!["é".as_bytes().to_vec(), "e\u{301}".as_bytes().to_vec()],
        ),
        ("reserved", vec![b"CON.txt".to_vec()]),
        ("long-name", vec![vec![b'n'; 240]]),
    ] {
        g.snapshot(
            "F02",
            name,
            paths
                .into_iter()
                .map(|p| (p, EntryKind::Regular, b"bytes\n".to_vec()))
                .collect(),
        )?;
    }
    let (file, mut objects) = g.snapshot(
        "F02",
        "transition-file",
        vec![(b"entry".to_vec(), EntryKind::Regular, b"file\n".to_vec())],
    )?;
    let target = if let Object::Tree(t) = Object::decode(&objects[&file])? {
        t.entries()[0].target.clone()
    } else {
        unreachable!()
    };
    let child = put(
        &mut objects,
        Object::Tree(Tree::new(vec![TreeEntry {
            name: b"nested".to_vec(),
            kind: EntryKind::Regular,
            target,
        }])?),
    )?;
    let directory = put(
        &mut objects,
        Object::Tree(Tree::new(vec![TreeEntry {
            name: b"entry".to_vec(),
            kind: EntryKind::Directory,
            target: Target::Native(child),
        }])?),
    )?;
    g.bundle("F02", "transition-directory", &objects, directory, 2)?;
    let mut deep = child;
    for _ in 0..40 {
        deep = put(
            &mut objects,
            Object::Tree(Tree::new(vec![TreeEntry {
                name: vec![b'd'; 32],
                kind: EntryKind::Directory,
                target: Target::Native(deep),
            }])?),
        )?;
    }
    g.bundle("F02", "long-path", &objects, deep, 2)?;
    g.cases.push(json!({"fixture":"F02","name":"file-directory-transition-and-long-path","before":file.to_string(),"after":directory.to_string(),"deep":deep.to_string(),"depth":41}));
    history(g)?;
    hostile(g)?;
    merges(g)?;
    Ok(())
}
fn history(g: &mut Generator) -> Result<()> {
    let (a, mut s) = g.snapshot(
        "F03",
        "state-a",
        vec![(b"file".to_vec(), EntryKind::Regular, b"A\n".to_vec())],
    )?;
    let (b, other) = g.snapshot(
        "F03",
        "state-b",
        vec![(b"file".to_vec(), EntryKind::Regular, b"B\n".to_vec())],
    )?;
    s.extend(other);
    let r = revision(&mut s, a, vec![], 1, 10)?;
    let rb = revision(&mut s, b, vec![r], 2, 10)?;
    let ra = revision(&mut s, a, vec![rb], 3, 9)?;
    let left = revision(&mut s, a, vec![r], 4, 11)?;
    let right = revision(&mut s, b, vec![r], 5, 11)?;
    let m1 = revision(&mut s, a, vec![left, right], 6, 12)?;
    let m2 = revision(&mut s, b, vec![right, left], 7, 12)?;
    let octopus = revision(&mut s, a, vec![m1, m2, ra], 8, 1)?;
    let disconnected = revision(&mut s, a, vec![], 9, -100)?;
    let rewrite = revision(&mut s, b, vec![r], 4, 11)?;
    let event = put(
        &mut s,
        Object::ChangeEvent(Box::new(ChangeEvent {
            kind: ChangeKind::Rewrite,
            change_ids: vec![[4; 16]],
            prior: vec![left],
            resulting: vec![rewrite],
            actor: person(12),
            reason: b"fixture rewrite".to_vec(),
            operation: None,
        })),
    )?;
    let view = put(
        &mut s,
        Object::View(Box::new(View {
            namespace_generation: 1,
            refs: vec![
                ViewRef {
                    name: "branches/disconnected".into(),
                    generation: 1,
                    state: RefState::Target(disconnected),
                },
                ViewRef {
                    name: "branches/main".into(),
                    generation: 1,
                    state: RefState::Target(octopus),
                },
                ViewRef {
                    name: "branches/rewrite".into(),
                    generation: 1,
                    state: RefState::Target(rewrite),
                },
            ],
            drafts: vec![],
            workspaces: vec![],
            retained: vec![RetainedRoot {
                type_code: 4,
                id: event,
            }],
        })),
    )?;
    g.bundle("F03", "history", &s, view, 6)?;
    g.cases.push(json!({"fixture":"F03","name":"history-dag","a_b_a":[r.to_string(),rb.to_string(),ra.to_string()],"tree_a":a.to_string(),"tree_b":b.to_string(),"criss_cross":[m1.to_string(),m2.to_string()],"merge_bases":[left.to_string(),right.to_string()],"octopus":octopus.to_string(),"disconnected":disconnected.to_string(),"rewrite":[left.to_string(),rewrite.to_string()],"view":view.to_string()}));
    Ok(())
}
fn hostile(g: &mut Generator) -> Result<()> {
    let blob = Blob {
        data: b"valid".to_vec(),
    }
    .encode()?;
    let mut bad_version = blob.clone();
    bad_version[12] = 2;
    let mut overflow = blob.clone();
    overflow[15..23].copy_from_slice(&u64::MAX.to_be_bytes());
    let mut trailing = blob.clone();
    trailing.push(0);
    for (name, b) in [
        ("truncated", blob[..22].to_vec()),
        ("wrong-version", bad_version),
        ("length-overflow", overflow),
        ("trailing", trailing),
        ("indefinite-cbor", frame(2, &[0x9f, 0xff])),
        ("nonminimal-cbor", frame(2, &[0x98, 0])),
    ] {
        if Object::decode(&b).is_ok() || kai_object_reference::decode(&b).is_ok() {
            return Err("hostile object accepted".into());
        }
        g.artifact(&format!("F06/{name}.object"), &b)?;
        g.cases
            .push(json!({"fixture":"F06","name":name,"expected":"reject-object-before-effects"}));
    }
    for (name, path) in [
        ("traversal", b"../outside".to_vec()),
        ("absolute", b"/outside".to_vec()),
        ("nul-name", b"a\0b".to_vec()),
    ] {
        if Tree::new(vec![TreeEntry {
            name: path.clone(),
            kind: EntryKind::Regular,
            target: Target::Native(ObjectId::hash(&blob)),
        }])
        .is_ok()
        {
            return Err("invalid tree name accepted".into());
        }
        g.artifact(&format!("F06/{name}.path"), &path)?;
        g.cases
            .push(json!({"fixture":"F06","name":name,"expected":"reject-path-before-effects"}));
    }
    // State-machine inputs for interfaces not implemented until P1/P2/P5.
    let pending = json!({"symlink_race":{"swap":"ancestor -> outside","expected":"no writes outside root"},"cycle":{"nodes":[[0,1],[1,0]],"expected":"reject DAG"},"type_confusion":{"parent_kind":"blob","expected":"reject required type"},"pack_bomb":{"declared_expansion":"18446744073709551615","expected":"reject before allocation"},"spoofed_receipt":{"transaction":"unknown","generation":"18446744073709551615","expected":"do not install roots"},"guessed_id":{"authorized":false,"expected":"deny object disclosure"}});
    g.artifact(
        "F06/pending-runtime-traces.json",
        &serde_json::to_vec_pretty(&pending).unwrap(),
    )?;
    g.cases.push(json!({"fixture":"F06","name":"pending-runtime-traces","expected":"specified inputs; production execution deferred"}));
    Ok(())
}
fn frame(kind: u16, payload: &[u8]) -> Vec<u8> {
    let mut b = b"KAI-OBJECT\0\0\x01".to_vec();
    b.extend(kind.to_be_bytes());
    b.extend((payload.len() as u64).to_be_bytes());
    b.extend(payload);
    b
}
fn merges(g: &mut Generator) -> Result<()> {
    type MergeInput = (&'static str, &'static [u8], &'static [u8], &'static [u8]);
    let cases: Vec<MergeInput> = vec![
        (
            "comments",
            b"// comment\nlet x = 1;\n",
            b"// changed\nlet x = 1;\n",
            b"// comment\nlet x = 2;\n",
        ),
        ("wrapper", b"f();\n", b"if (ready) { f(); }\n", b"g();\n"),
        (
            "imports",
            b"import a\nf()\n",
            b"import a\nimport b\nf()\n",
            b"import a\ng()\n",
        ),
        (
            "directives",
            b"#if X\nf();\n#endif\n",
            b"#if Y\nf();\n#endif\n",
            b"#if X\ng();\n#endif\n",
        ),
        (
            "top-level",
            b"register();\n",
            b"register();\nstart();\n",
            b"setup();\nregister();\n",
        ),
        ("binary", b"\0\xffA", b"\0\xffB", b"\0\xffC"),
        (
            "unsupported",
            b"%%% unknown syntax\n",
            b"%%% left malformed {\n",
            b"%%% right malformed ]\n",
        ),
    ];
    for (name, base, left, right) in cases {
        let mut ids = vec![];
        for (side, data) in [("base", base), ("left", left), ("right", right)] {
            let (tree, _) = g.snapshot(
                "F07",
                &format!("{name}-{side}"),
                vec![(b"source".to_vec(), EntryKind::Regular, data.to_vec())],
            )?;
            ids.push(tree.to_string());
        }
        g.cases.push(json!({"fixture":"F07","name":format!("merge-{name}"),"trees":ids,"merge_execution":"pending P4; never infer clean from parser coverage"}));
    }
    let real = include_bytes!("../corpus/kai-lib.rs.txt");
    let left = [b"// fixture: left documentation edit\n".as_slice(), real].concat();
    let right = [real.as_slice(), b"\npub const FIXTURE_RIGHT: u8 = 1;\n"].concat();
    let mut real_ids = vec![];
    for (side, data) in [
        ("base", real.as_slice()),
        ("left", left.as_slice()),
        ("right", right.as_slice()),
    ] {
        let (tree, _) = g.snapshot(
            "F07",
            &format!("real-kai-{side}"),
            vec![(b"lib.rs".to_vec(), EntryKind::Regular, data.to_vec())],
        )?;
        real_ids.push(tree.to_string());
    }
    g.artifact(
        "F07/real-source.LICENSE",
        include_bytes!("../corpus/LICENSE"),
    )?;
    g.cases.push(json!({"fixture":"F07","name":"real-kai-edits","source_commit":"764948bea13c859f29d64af9153d6f6b27527082","source_path":"src/lib.rs","license":"MIT","trees":real_ids,"merge_execution":"pending P4"}));
    for (side, files) in [
        (
            "base",
            vec![(b"old".to_vec(), EntryKind::Regular, b"same\n".to_vec())],
        ),
        (
            "left",
            vec![(b"new".to_vec(), EntryKind::Executable, b"same\n".to_vec())],
        ),
        ("right", vec![]),
    ] {
        g.snapshot("F07", &format!("rename-mode-delete-{side}"), files)?;
    }
    g.cases.push(json!({"fixture":"F07","name":"rename-mode-delete","expected":"pending P4: preserve rename/mode and delete alternatives"}));
    Ok(())
}
