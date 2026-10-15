mod grammars;

use super::detect;

fn check(cases: &[(&str, &str, &str)]) {
    for (name, head, expected) in cases {
        assert_eq!(
            detect(name, head.as_bytes()),
            *expected,
            "{name:?} with {head:?}"
        );
    }
}

#[test]
fn existing_extensions_keep_their_language() {
    check(&[
        ("main.rs", "", "rust"),
        ("App.TSX", "", "tsx"),
        ("Main.JAVA", "", "java"),
        ("Makefile", "", "make"),
        ("mix.exs", "", "elixir"),
        ("notes.md", "", "markdown"),
        ("config.yml", "", "yaml"),
        ("Cargo.toml", "", "toml"),
        ("run.sh", "", "bash"),
        ("lib.h", "", "c"),
        ("view.hpp", "", "cpp"),
        ("app.mjs", "", "javascript"),
        ("Program.cs", "", "csharp"),
        ("page.htm", "", "html"),
    ]);
}

#[test]
fn new_extensions_pick_their_language() {
    let xml = [
        "a.xml",
        "a.xaml",
        "a.svg",
        "a.plist",
        "a.csproj",
        "a.vcxproj",
        "a.props",
        "a.targets",
        "a.resx",
        "a.nuspec",
        "a.xsd",
        "a.xsl",
        "a.xslt",
        "app.config",
        "ecu.arxml",
    ];
    for name in xml {
        assert_eq!(detect(name, b""), "xml", "{name}");
    }
    check(&[
        ("query.sql", "", "sql"),
        ("build.ps1", "", "powershell"),
        ("tools.psm1", "", "powershell"),
        ("tools.psd1", "", "powershell"),
        ("setup.ini", "", "ini"),
        ("driver.inf", "", "ini"),
        ("app.properties", "", "properties"),
        ("build.bat", "", "batch"),
        ("build.CMD", "", "batch"),
        ("web.dockerfile", "", "dockerfile"),
        ("data.json5", "", "json5"),
        ("tsconfig.jsonc", "", "json"),
        ("plot.R", "", "r"),
        ("tool.pl", "", "perl"),
        ("Tool.pm", "", "perl"),
        ("paper.tex", "", "latex"),
        ("style.sty", "", "latex"),
        ("bridge.mm", "", "objc"),
        ("Form1.vb", "", "vb"),
        ("setup.vbs", "", "vb"),
        ("Module1.bas", "", "vb"),
        ("boot.s", "", "asm"),
        ("boot.S", "", "asm"),
        ("start.asm", "", "asm"),
    ]);
}

#[test]
fn special_names_pick_their_language() {
    check(&[
        ("GNUmakefile", "", "make"),
        ("Dockerfile", "", "dockerfile"),
        ("Containerfile", "", "dockerfile"),
        ("Dockerfile.prod", "", "dockerfile"),
        ("CMakeLists.txt", "", "cmake"),
    ]);
}

#[test]
fn an_xml_prolog_gives_xml() {
    check(&[
        ("export.txt", "<?xml version=\"1.0\"?>\n<a/>", "xml"),
        ("export", "\u{feff}<?xml version=\"1.0\"?>", "xml"),
        ("export.dat", "  <?xml version=\"1.0\"?>", "xml"),
    ]);
}

#[test]
fn shebang_lines_give_their_language() {
    check(&[
        ("tool", "#!/usr/bin/env python3\nprint(1)", "python"),
        ("tool", "#!/usr/bin/python3.12", "python"),
        ("tool", "#!/usr/bin/env -S python3 -u", "python"),
        ("tool", "#!/bin/bash\necho hi", "bash"),
        ("tool", "#!/bin/sh", "bash"),
        ("tool", "#!/usr/bin/perl -w", "perl"),
        ("tool", "#!/usr/bin/env pwsh", "powershell"),
        ("tool", "#!/usr/bin/env Rscript", "r"),
        ("tool", "#!/usr/bin/env node", "javascript"),
        ("tool", "#!/usr/bin/unknown", "text"),
    ]);
}

#[test]
fn first_line_mode_hints_give_their_language() {
    check(&[
        ("notes", "# -*- mode: python -*-", "python"),
        ("notes", "# -*- coding: utf-8; mode: perl -*-", "perl"),
        ("notes", "/* -*- c++ -*- */", "cpp"),
        ("notes", "# vim: set ft=sh :", "bash"),
        ("notes", "// vim: ft=xml", "xml"),
        ("notes", "# vi: filetype=dosini", "ini"),
        ("notes", "#!/usr/bin/unknown\n# vim: ft=python", "python"),
        ("notes", "# -*- mode: nonsense -*-", "text"),
    ]);
}

#[test]
fn a_known_extension_wins_over_content() {
    check(&[
        ("main.rs", "<?xml version=\"1.0\"?>", "rust"),
        ("run.sh", "#!/usr/bin/env python3", "bash"),
    ]);
}

#[test]
fn dot_m_is_objective_c_or_text() {
    check(&[
        ("App.m", "#import <Foundation/Foundation.h>\n", "objc"),
        ("App.m", "// view\n@implementation App\n@end\n", "objc"),
        (
            "solve.m",
            "function y = solve(x)\n  y = x * 2;\nend\n",
            "text",
        ),
    ]);
}

#[test]
fn dot_cls_is_latex_or_vb() {
    check(&[
        (
            "thesis.cls",
            "\\NeedsTeXFormat{LaTeX2e}\n\\ProvidesClass{thesis}",
            "latex",
        ),
        ("report.cls", "\\documentclass{article}", "latex"),
        ("Sheet.cls", "VERSION 1.0 CLASS\nBEGIN\nEND", "vb"),
        ("Sheet.cls", "Attribute VB_Name = \"Sheet\"\n", "vb"),
        ("Account.cls", "public class Account {}", "text"),
    ]);
}

#[test]
fn dot_inc_is_decided_by_content() {
    check(&[
        ("header.inc", "<?php\necho 1;", "php"),
        ("header.inc", "#define LIMIT 10\n", "c"),
        ("macros.inc", "; macros\n%macro save 0\n", "asm"),
        ("parts.inc", "plain words", "text"),
    ]);
}

#[test]
fn dot_conf_is_decided_by_content() {
    check(&[
        ("app.conf", "# app\n[main]\nlevel = 2\n", "ini"),
        ("app.conf", "# app\nlevel = 2\n", "properties"),
        ("site.conf", "server {\n  listen 80;\n}\n", "text"),
        ("site.conf", "<?xml version=\"1.0\"?>", "xml"),
    ]);
}

#[test]
fn unknown_files_stay_text() {
    check(&[
        ("readme", "", "text"),
        (".bashrc", "", "text"),
        ("app.log", "started", "text"),
        ("notes.txt", "hello", "text"),
    ]);
}
