use std::cell::RefCell;

use navo_index::pipe::{SERVICE_REVISION, ServiceVersion};

use super::*;

const PROCESS: u32 = 40;

fn known(revision: u32) -> ServiceVersion {
    ServiceVersion::Known {
        revision,
        build: "b".into(),
    }
}

fn seen(version: &ServiceVersion, server: u32) -> Seen<'_> {
    Seen {
        reply: Some((version, server)),
        alive: true,
        scm: PROCESS,
    }
}

#[test]
fn the_retained_process_answering_the_current_revision_passes() {
    let version = known(SERVICE_REVISION);
    assert!(matches!(
        check_answer(PROCESS, &seen(&version, PROCESS)),
        Ok(true)
    ));
}

#[test]
fn the_service_revision_is_three() {
    assert_eq!(SERVICE_REVISION, 3);
}

#[test]
fn each_way_the_answer_misses_the_retained_process_is_a_mismatch() {
    let current = known(SERVICE_REVISION);
    let (older, two) = (ServiceVersion::Older, known(2));
    let exited = Seen {
        alive: false,
        ..seen(&current, PROCESS)
    };
    let moved = Seen {
        scm: PROCESS + 1,
        ..seen(&current, PROCESS)
    };
    assert!(matches!(
        check_answer(PROCESS, &seen(&current, PROCESS + 1)),
        Err(Mismatch::OtherServer { .. })
    ));
    assert!(matches!(
        check_answer(PROCESS, &exited),
        Err(Mismatch::Exited)
    ));
    assert!(matches!(
        check_answer(PROCESS, &moved),
        Err(Mismatch::Moved { .. })
    ));
    assert!(matches!(
        check_answer(PROCESS, &seen(&older, PROCESS)),
        Err(Mismatch::Revision(_))
    ));
    assert!(matches!(
        check_answer(PROCESS, &seen(&two, PROCESS)),
        Err(Mismatch::Revision(_))
    ));
    for error in [Mismatch::Exited, Mismatch::Revision("Older".into())] {
        assert!(is_mismatch(&io::Error::other(error)));
    }
}

#[test]
fn a_silent_pipe_waits_while_the_process_runs_and_is_a_mismatch_once_it_exited() {
    let silent = Seen {
        reply: None,
        alive: true,
        scm: PROCESS,
    };
    assert!(matches!(check_answer(PROCESS, &silent), Ok(false)));
    let gone = Seen {
        alive: false,
        ..silent
    };
    assert!(matches!(
        check_answer(PROCESS, &gone),
        Err(Mismatch::Exited)
    ));
    let stopped = Seen { scm: 0, ..silent };
    assert!(matches!(
        check_answer(PROCESS, &stopped),
        Err(Mismatch::Moved { .. })
    ));
}

#[test]
fn the_install_copies_the_sibling_service_image_and_refuses_when_it_is_missing() {
    let root = tempfile::tempdir().unwrap();
    let app = root.path().join("navo.exe");
    std::fs::write(&app, b"app").unwrap();
    let error = service_source(&app).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert!(error.to_string().contains("navo-index.exe"));
    std::fs::create_dir(root.path().join("navo-index.exe")).unwrap();
    assert!(service_source(&app).is_err(), "a folder is not the image");
    std::fs::remove_dir(root.path().join("navo-index.exe")).unwrap();
    std::fs::write(root.path().join("navo-index.exe"), b"service").unwrap();
    assert_eq!(
        service_source(&app).unwrap(),
        root.path().join("navo-index.exe")
    );
}

#[test]
fn the_service_command_runs_the_quoted_index_image() {
    let image = Path::new(r"C:\Program Files\Navo\Index\navo-index.exe");
    assert_eq!(
        service_command(image),
        OsStr::new(r#""C:\Program Files\Navo\Index\navo-index.exe" run"#)
    );
}

#[test]
fn delete_legacy_removes_only_the_old_image_and_its_old_copies() {
    let deleted = RefCell::new(Vec::new());
    let names = [
        "navo-index.exe",
        "NAVO.EXE",
        "navo.exe.12.old",
        "navo.exe.x.old",
        "navo.exe.3.tmp",
        "navo-index.exe.4.old",
        "notes.txt",
    ];
    let paths = names.map(|name| PathBuf::from("Index").join(name));
    let result = delete_legacy(
        paths,
        |path| {
            deleted
                .borrow_mut()
                .push(path.file_name().unwrap().to_owned());
            Ok(())
        },
        |_| panic!("nothing refused"),
    );
    assert!(result.is_ok());
    assert_eq!(deleted.into_inner(), ["NAVO.EXE", "navo.exe.12.old"]);
}

#[test]
fn a_refused_legacy_delete_is_scheduled_and_reported_as_a_warning() {
    let scheduled = RefCell::new(Vec::new());
    let paths = ["navo.exe", "navo.exe.1.old", "navo.exe.2.old"]
        .map(|name| PathBuf::from("Index").join(name));
    let warning = delete_legacy(
        paths,
        |path| match path.file_name().unwrap().to_str().unwrap() {
            "navo.exe" => Err(io::ErrorKind::PermissionDenied.into()),
            "navo.exe.1.old" => Err(io::ErrorKind::NotFound.into()),
            _ => Err(io::Error::other("in use")),
        },
        |path| {
            scheduled.borrow_mut().push(path.to_path_buf());
            if path.ends_with("navo.exe.2.old") {
                Err(io::Error::other("schedule refused"))
            } else {
                Ok(())
            }
        },
    )
    .unwrap_err()
    .to_string();
    assert_eq!(
        scheduled.borrow().len(),
        2,
        "a vanished file needs no schedule"
    );
    assert!(
        warning.contains("navo.exe is deleted at the next restart"),
        "{warning}"
    );
    assert!(warning.contains("schedule refused"), "{warning}");
}
