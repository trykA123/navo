use super::Group;

pub(super) const GROUPS: [Group; 1] = [Group {
    context: "FileList",
    commands: &[(
        "file_list::CopyPath",
        "files.copy_path",
        "cmd-files-copy-path",
    )],
}];
