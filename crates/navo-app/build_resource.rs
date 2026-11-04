struct Resource<'a> {
    icon: &'a str,
    version: &'a str,
    product_version: &'a str,
    description: &'a str,
    internal_name: &'a str,
    original_filename: &'a str,
}

fn resource_script(resource: Resource<'_>) -> String {
    let Resource {
        icon,
        version,
        product_version,
        description,
        internal_name,
        original_filename,
    } = resource;
    let mut parts = version
        .split(['.', '-'])
        .map(|part| part.parse::<u16>().unwrap_or(0));
    let mut next = || parts.next().unwrap_or(0);
    let numeric = format!("{},{},{},0", next(), next(), next());
    let icon = icon.replace('\\', "\\\\");
    format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {numeric}
PRODUCTVERSION {numeric}
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "{description}"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "{internal_name}"
      VALUE "OriginalFilename", "{original_filename}"
      VALUE "ProductName", "{description}"
      VALUE "ProductVersion", "{product_version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#
    )
}
