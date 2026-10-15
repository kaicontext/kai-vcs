use super::*;
fn run(repo: &Path, args: &[&str], input: &[u8]) -> Result<Vec<u8>> {
    let mut cmd = Command::new("git");
    cmd.arg("-C")
        .arg(repo)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(key);
        }
    }
    cmd.env("GIT_CONFIG_NOSYSTEM", "1")
        .env(
            "GIT_CONFIG_GLOBAL",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
        )
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("LC_ALL", "C")
        .env("GIT_NO_REPLACE_OBJECTS", "1");
    let mut child = cmd.spawn().map_err(|e| e.to_string())?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input)
        .map_err(|e| e.to_string())?;
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(output.stdout)
}
fn text(repo: &Path, args: &[&str], input: &[u8]) -> Result<String> {
    String::from_utf8(run(repo, args, input)?)
        .map(|s| s.trim().to_owned())
        .map_err(|e| e.to_string())
}
fn raw(
    g: &mut Generator,
    repo: &Path,
    algorithm: &str,
    name: &str,
    kind: &str,
    data: &[u8],
) -> Result<String> {
    let id = text(repo, &["hash-object", "-w", "-t", kind, "--stdin"], data)?;
    if run(repo, &["cat-file", kind, &id], &[])? != data {
        return Err("upstream Git raw roundtrip mismatch".into());
    }
    g.artifact(&format!("F04/{algorithm}/{name}.{kind}"), data)?;
    Ok(id)
}
fn commit(tree: &str, parent: Option<&str>, extra: &[u8], message: &[u8]) -> Vec<u8> {
    let mut b = format!("tree {tree}\n").into_bytes();
    if let Some(p) = parent {
        b.extend(format!("parent {p}\n").as_bytes());
    }
    b.extend(b"author Fixture <fixture@example.test> 100 +0130\ncommitter Fixture <fixture@example.test> 100 -0200\n");
    b.extend(extra);
    b.push(b'\n');
    b.extend(message);
    b
}
pub(super) fn generate(g: &mut Generator) -> Result<()> {
    for algorithm in ["sha1", "sha256"] {
        let repo = g.root.join(format!("F04/{algorithm}/repository"));
        fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
        run(
            &repo,
            &[
                "init",
                "--bare",
                "--initial-branch=main",
                &format!("--object-format={algorithm}"),
            ],
            &[],
        )?;
        let blob = raw(g, &repo, algorithm, "binary", "blob", b"\0\xffsource\r\n")?;
        let tree = text(
            &repo,
            &["mktree", "-z"],
            format!("100644 blob {blob}\tfile\0").as_bytes(),
        )?;
        let base = raw(
            g,
            &repo,
            algorithm,
            "unsigned",
            "commit",
            &commit(&tree, None, b"", b"root\n"),
        )?;
        let signed=raw(g,&repo,algorithm,"signed-armor","commit",&commit(&tree,Some(&base),b"encoding ISO-8859-1\nx-fixture opaque\ngpgsig -----BEGIN PGP SIGNATURE-----\n fixture-only-not-a-valid-signature\n -----END PGP SIGNATURE-----\n",b"opaque \xff message\n"))?;
        let tag=raw(g,&repo,algorithm,"unsigned","tag",format!("object {signed}\ntype commit\ntag v1\ntagger Fixture <fixture@example.test> 100 +0000\n\nunsigned tag\n").as_bytes())?;
        let signed_tag=raw(g,&repo,algorithm,"signed-armor","tag",format!("object {signed}\ntype commit\ntag signed\ntagger Fixture <fixture@example.test> 100 +0000\n\ntag\n-----BEGIN PGP SIGNATURE-----\nfixture-only-not-a-valid-signature\n-----END PGP SIGNATURE-----\n").as_bytes())?;
        let note = raw(g, &repo, algorithm, "note", "blob", b"fixture note\n")?;
        let note_tree = text(
            &repo,
            &["mktree", "-z"],
            format!("100644 blob {note}\t{base}\0").as_bytes(),
        )?;
        let notes = raw(
            g,
            &repo,
            algorithm,
            "notes",
            "commit",
            &commit(&note_tree, None, b"", b"notes\n"),
        )?;
        for (name, id) in [
            ("refs/heads/main", &signed),
            ("refs/tags/v1", &tag),
            ("refs/tags/signed", &signed_tag),
            ("refs/custom/fixture", &base),
            ("refs/notes/fixture", &notes),
        ] {
            run(&repo, &["update-ref", name, id], &[])?;
        }
        let gitlink_tree = text(
            &repo,
            &["mktree", "-z"],
            format!("160000 commit {base}\tsubmodule\0").as_bytes(),
        )?;
        let gitlink = raw(
            g,
            &repo,
            algorithm,
            "gitlink",
            "commit",
            &commit(&gitlink_tree, Some(&signed), b"", b"gitlink\n"),
        )?;
        run(&repo, &["update-ref", "refs/heads/gitlink", &gitlink], &[])?;
        let alt = g.root.join(format!("F04/{algorithm}/alternate-client"));
        fs::create_dir(&alt).map_err(|e| e.to_string())?;
        run(
            &alt,
            &["init", "--bare", &format!("--object-format={algorithm}")],
            &[],
        )?;
        fs::write(
            alt.join("objects/info/alternates"),
            b"../../repository/objects\n",
        )
        .map_err(|e| e.to_string())?;
        if text(&alt, &["cat-file", "-t", &base], &[])? != "commit" {
            return Err("alternate object unavailable".into());
        }
        fs::write(repo.join("shallow"), format!("{signed}\n")).map_err(|e| e.to_string())?;
        run(
            &repo,
            &["update-ref", &format!("refs/replace/{signed}"), &base],
            &[],
        )?;
        fs::write(repo.join("info/grafts"), format!("{gitlink} {base}\n"))
            .map_err(|e| e.to_string())?;
        let refs = text(
            &repo,
            &["for-each-ref", "--format=%(refname) %(objectname)"],
            &[],
        )?;
        g.artifact(
            &format!("F04/{algorithm}/expected-refs.txt"),
            refs.as_bytes(),
        )?;
        g.cases.push(json!({"fixture":"F04","name":algorithm,"root":base,"signed_armor":signed,"tag":tag,"signed_tag_armor":signed_tag,"gitlink":gitlink,"tree":tree,"gitlink_tree":gitlink_tree,"blob":blob,"alternates":"relative path to sibling repository","shallow_boundary":signed,"replacement":[signed,base],"graft":[gitlink,base],"signature_status":"synthetic armor; raw preservation only, not cryptographic verification"}));
        if algorithm == "sha1" {
            working(g, &repo, &base, &signed)?;
        }
    }
    Ok(())
}
fn working(g: &mut Generator, bare: &Path, base: &str, signed: &str) -> Result<()> {
    let repo = g.root.join("F05/worktree");
    fs::create_dir_all(&repo).map_err(|e| e.to_string())?;
    run(&repo, &["init", "--initial-branch=main"], &[])?;
    // Internalize fixture objects via local pack transfer; never run filters/hooks.
    let pack = run(
        bare,
        &["pack-objects", "--stdout", "--revs"],
        format!("{base}\n{signed}\n").as_bytes(),
    )?;
    run(&repo, &["index-pack", "--stdin"], &pack)?;
    run(&repo, &["update-ref", "refs/heads/main", base], &[])?;
    let staged = b"line one staged\nline two base\n";
    let working = b"line one staged\nline two unstaged\n";
    let blob = text(&repo, &["hash-object", "-w", "--stdin"], staged)?;
    run(
        &repo,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{blob},file"),
        ],
        &[],
    )?;
    for (name, data) in [
        ("file", working.as_slice()),
        (".gitignore", b"ignored\n"),
        ("ignored", b"private ignored bytes\n"),
        ("untracked", b"unsaved untracked bytes\n"),
        ("intent", b"intent-to-add bytes\n"),
    ] {
        fs::write(repo.join(name), data).map_err(|e| e.to_string())?;
        g.artifact(&format!("F05/expected/{name}.bytes"), data)?;
    }
    run(&repo, &["add", "--intent-to-add", "intent"], &[])?;
    let sparse = text(
        &repo,
        &["hash-object", "-w", "--stdin"],
        b"sparse saved bytes\n",
    )?;
    run(
        &repo,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("100644,{sparse},sparse"),
        ],
        &[],
    )?;
    run(&repo, &["update-index", "--skip-worktree", "sparse"], &[])?;
    run(&repo, &["config", "core.sparseCheckout", "true"], &[])?;
    fs::write(repo.join(".git/info/sparse-checkout"), b"/file\n/intent\n")
        .map_err(|e| e.to_string())?;
    run(
        &repo,
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            &format!("160000,{base},sub"),
        ],
        &[],
    )?;
    let sub = repo.join("sub");
    fs::create_dir(&sub).map_err(|e| e.to_string())?;
    run(&sub, &["init", "--initial-branch=main"], &[])?;
    let pack = run(
        bare,
        &["pack-objects", "--stdout", "--revs"],
        format!("{base}\n").as_bytes(),
    )?;
    run(&sub, &["index-pack", "--stdin"], &pack)?;
    run(&sub, &["update-ref", "refs/heads/main", base], &[])?;
    fs::write(sub.join("dirty"), b"dirty submodule bytes\n").map_err(|e| e.to_string())?;
    fs::write(repo.join(".git/HEAD"), format!("{base}\n")).map_err(|e| e.to_string())?;
    fs::write(repo.join(".git/MERGE_HEAD"), format!("{signed}\n")).map_err(|e| e.to_string())?;
    fs::write(repo.join(".git/MERGE_MSG"), b"fixture merge in progress\n")
        .map_err(|e| e.to_string())?;
    let index = text(&repo, &["ls-files", "--stage"], &[])?;
    let flags = text(&repo, &["ls-files", "-v"], &[])?;
    g.artifact("F05/expected/index.txt", index.as_bytes())?;
    g.artifact("F05/expected/flags.txt", flags.as_bytes())?;
    g.artifact("F05/expected/staged.bytes", staged)?;
    g.cases.push(json!({"fixture":"F05","name":"working-state","head":base,"head_state":"detached","merge_head":signed,"staged_blob":blob,"sparse_blob":sparse,"gitlink":base,"expected":"staged/unstaged overlap, partial-line selection, ignored/untracked, absent sparse, intent-to-add, dirty submodule, active merge","capture_execution":"pending P2/P6; fixture state inspected with upstream Git"}));
    g.snapshot(
        "F05",
        "checkout-collisions",
        vec![
            (
                b"ignored".to_vec(),
                EntryKind::Regular,
                b"tracked incoming ignored path\n".to_vec(),
            ),
            (
                b"untracked".to_vec(),
                EntryKind::Regular,
                b"tracked incoming untracked path\n".to_vec(),
            ),
        ],
    )?;
    Ok(())
}
