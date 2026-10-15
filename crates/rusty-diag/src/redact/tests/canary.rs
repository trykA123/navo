use super::{Identity, Redactor, SALT_A, Salt};

const LINES: [&str; 20] = [
    r"session start user=canary-user-7731 host=CANARY-PC-7731 domain=CANARYDOM",
    r"settings loaded from C:\Users\canary-user-7731\AppData\Roaming\Ferrite\ferrite.toml",
    r"open folder C:\Users\canary-user-7731\Documents\canary-project-7731",
    r"thumbnail failed for C:\Users\canary-user-7731\Pictures\canary_7731_beach.JPG: decode error",
    r"listing \\canary-srv-7731\share\Reports\2026 took 412 ms (network)",
    r"connection to canary-srv-7731 reset by peer, retrying",
    r"copy job 12: \\canary-srv-7731\share\canary-7731.zip -> C:\Users\canary-user-7731\Downloads\canary-7731.zip",
    r"Access is denied: 'C:\Users\canary-user-7731\Desktop\canary-secret-7731.txt'",
    r#"The process cannot access the file "C:\Users\canary-user-7731\canary.db" because it is being used by another process."#,
    r"lock owner: CANARYDOM\canary-user-7731 (pid 4412)",
    r"profile root c:\users\CANARY-USER-7731 resolved",
    r"watcher started on c:/users/canary-user-7731/.cargo/registry",
    r"long path \\?\C:\Users\canary-user-7731\canary7731\deep\nested\file-7731.bin",
    r"UNC long path \\?\UNC\canary-srv-7731\share\canary-7731\q3.xlsx",
    r"mapped Z: -> \\CANARY-SRV-7731\share",
    r"kerberos principal canary-user-7731@CANARYDOM.LOCAL on canary-pc-7731.canarydom.local",
    r"recent: C:\Users\canary-user-7731\OneDrive - CanaryDom\q3-report.docx",
    r"drop target <C:\Users\canary-user-7731\Videos\canary-7731.mkv>",
    r"rename C:\Users\canary-user-7731\a.txt|C:\Users\canary-user-7731\canary-7731.txt",
    r"shutdown: CANARY-PC-7731 idle, canary-user-7731 logged off",
];

fn canary_redactor() -> Redactor {
    let identity = Identity {
        user: Some("canary-user-7731".to_owned()),
        computer: Some("CANARY-PC-7731".to_owned()),
        domain: Some("CANARYDOM".to_owned()),
        profile: Some(r"C:\Users\canary-user-7731".to_owned()),
    };
    Redactor::new(Salt::from_bytes(SALT_A), identity)
}

#[test]
fn canary_values_never_reach_the_output() {
    let r = canary_redactor();
    let leaks: Vec<String> = LINES
        .iter()
        .map(|line| r.text(line))
        .filter(|out| {
            let lower = out.to_lowercase();
            ["canary", "7731", "canarydom"]
                .iter()
                .any(|c| lower.contains(c))
        })
        .collect();
    assert!(leaks.is_empty(), "canary leaked: {leaks:#?}");
}
