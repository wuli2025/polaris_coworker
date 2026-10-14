use super::*;
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Vector {
    name: String, owner: Owner, host: String, process: ProcessState,
    now: u64, max_hold_ms: u64, reclaimable: bool,
}
#[test]
fn shared_decisions() -> Result<(), Box<dyn std::error::Error>> {
    let rows: Vec<Vector> = serde_json::from_str(include_str!("../../tests/fixtures/credential-lock-vectors.json"))?;
    for row in rows {
        assert_eq!(lock_owner::reclaimable(&row.owner, &row.host, &row.process, row.now, row.max_hold_ms), row.reclaimable, "{}", row.name);
    }
    Ok(())
}
#[test]
fn slow_owner_and_concurrent_reclamation() -> Result<(), String> {
    let dir = std::env::temp_dir().join(format!("r5-lock-{}", nonce()?));
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join("providers.json");
    let old = FileLock::acquire_with(&path, 1000, 1, 600000)?;
    assert!(matches!(FileLock::acquire_with(&path, 20, 1, 600000), Err(e) if e == "CredentialStoreBusy"));
    old.assert_owned()?;
    let owner_path = old.lock.join(&old.token).join("owner.json");
    let mut info = lock_owner::owner()?;
    info.process_start = "dead-previous-incarnation".into();
    fs::write(owner_path, serde_json::to_vec(&info).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    let barrier = Arc::new(std::sync::Barrier::new(3));
    let workers: Vec<_> = (0..2).map(|_| {
        let lock = old.lock.clone();
        let barrier = barrier.clone();
        thread::spawn(move || { barrier.wait(); reclaim(&lock, 600000) })
    }).collect();
    barrier.wait();
    let mut wins = 0;
    for worker in workers { if worker.join().map_err(|_| "ReaperPanicked")?? { wins += 1; } }
    assert_eq!(wins, 1);
    let next = FileLock::acquire_with(&path, 1000, 1, 600000)?;
    drop(old);
    next.assert_owned()?;
    drop(next);
    fs::remove_dir_all(dir).map_err(|e| e.to_string())
}
#[cfg(unix)]
#[test]
fn killed_node_owner_is_recovered_by_rust() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Read;
    let dir = std::env::temp_dir().join(format!("r5-cross-{}", nonce()?));
    fs::create_dir_all(&dir)?;
    let path = dir.join("providers.json");
    let agentd = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../agentd");
    let mut child = Command::new("node").args(["--import", "tsx", "--input-type=module", "-e",
        "import { acquire } from './src/models/credential-lock.ts'; await acquire(process.argv[1]+'.lock',{timeoutMs:2000,pollMs:1}); process.stdout.write('!'); setInterval(()=>{},1000);"])
        .arg(&path).current_dir(agentd).stdout(Stdio::piped()).spawn()?;
    let mut stdout = child.stdout.take().ok_or("MissingChildStdout")?;
    let mut ready = [0];
    let ready_result = stdout.read_exact(&mut ready);
    child.kill()?;
    child.wait()?;
    ready_result?;
    assert_eq!(ready, [b'!']);
    let lock = FileLock::acquire_with(&path, 1000, 1, 600000)?;
    lock.assert_owned()?;
    drop(lock);
    fs::remove_dir_all(dir)?;
    Ok(())
}
