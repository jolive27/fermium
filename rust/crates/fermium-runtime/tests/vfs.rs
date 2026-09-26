//! vfs: files written during a run are remembered (the playground shows them), stderr can be captured.
use fermium_runtime::vfs;

#[test]
fn written_files_are_listed_once_in_order() {
    let dir = std::env::temp_dir().join(format!("fermium-vfs-{}", std::process::id()));
    let a = dir.join("sub/a.svg").to_string_lossy().into_owned();
    let b = dir.join("b.png").to_string_lossy().into_owned();
    let _ = vfs::take_written();
    vfs::write(&a, b"<svg/>").unwrap();
    vfs::write(&b, b"png").unwrap();
    vfs::write(&a, b"<svg></svg>").unwrap();
    vfs::put(&dir.join("data.csv").to_string_lossy(), b"x\n1\n").unwrap(); // input files aren't listed
    assert_eq!(vfs::take_written(), vec![a.clone(), b]);
    assert!(vfs::take_written().is_empty());
    assert_eq!(vfs::read_to_string(&a).unwrap(), "<svg></svg>");
    assert!(vfs::read_to_string(&dir.join("nope").to_string_lossy()).is_err());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn stderr_capture() {
    vfs::capture_stderr();
    vfs::stderr_line("warning: one");
    vfs::stderr_line("warning: two");
    assert_eq!(vfs::take_stderr(), "warning: one\nwarning: two\n");
    assert_eq!(vfs::take_stderr(), "");
}
