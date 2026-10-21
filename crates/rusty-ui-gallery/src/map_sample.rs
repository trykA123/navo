use std::fmt::Write as _;
use std::sync::LazyLock;

pub const MAP_NAME: &str = "bcm_app.map";

pub const MAP_STEPS: [&str; 6] = [
    "viewer::LeftDrawer",
    "viewer::RowDown",
    "viewer::RowDown",
    "viewer::RowDown",
    "map_view::PickItem",
    "viewer::RightDrawer",
];

pub const MAP_UNREAD_STEPS: [&str; 2] = ["map_view::NextTable", "map_view::NextTable"];

static MAP: LazyLock<String> = LazyLock::new(linker_map);

static MAP_UNREAD: LazyLock<String> = LazyLock::new(unread_map);

pub fn map() -> &'static str {
    &MAP
}

pub fn map_unread() -> &'static str {
    &MAP_UNREAD
}

const SPACE: &str = "mpe:vtc:linear";

const MODULES: [(&str, &str, [&str; 3]); 14] = [
    ("can_drv", "CanDrv", ["Init", "MainFunction", "Transmit"]),
    ("lin_drv", "LinDrv", ["Init", "Wakeup", "SendFrame"]),
    ("door_ctrl", "Doors", ["Scan", "Lock", "Unlock"]),
    ("window_lift", "Window", ["Move", "Stop", "Calibrate"]),
    ("light_ctrl", "Lights", ["Init", "Update", "Fade"]),
    ("wiper", "Wiper", ["Step", "Park", "Rain"]),
    ("immobilizer", "Immo", ["Check", "Learn", "Reset"]),
    ("diag_uds", "Uds", ["Dispatch", "ReadData", "Session"]),
    ("nvm_mgr", "Nvm", ["ReadBlock", "WriteBlock", "Format"]),
    ("os_kernel", "Os", ["Start", "Schedule", "Tick"]),
    ("adc_drv", "Adc", ["Init", "Convert", "Result"]),
    ("pwm_drv", "Pwm", ["Init", "SetDuty", "Stop"]),
    ("wdg_mgr", "Wdg", ["Init", "Trigger", "Check"]),
    ("boot_cfg", "Boot", ["Entry", "Verify", "Jump"]),
];

const RESERVED: [u64; 4] = [0x8000, 0x2000, 0, 0];

const REGIONS: [(&str, u64, u64); 4] = [
    ("mpe:pflash0", 0x8000_0000, 0x20_0000),
    ("mpe:dflash0", 0xAF00_0000, 0x1_0000),
    ("mpe:dsram0", 0x7000_0000, 0x2_0000),
    ("mpe:psram0", 0x7010_0000, 0x1_0000),
];

struct Part {
    object: String,
    input: String,
    size: u64,
    offset: u64,
    output: String,
}

struct Output {
    name: String,
    region: usize,
    address: u64,
    size: u64,
    group: &'static str,
}

struct Symbol {
    name: String,
    address: u64,
    size: u64,
}

struct Builder {
    state: u32,
    cursors: [u64; 4],
    used: [u64; 4],
    outputs: Vec<Output>,
    parts: Vec<Part>,
    symbols: Vec<Symbol>,
}

impl Builder {
    fn next(&mut self, low: u64, span: u64) -> u64 {
        self.state = self
            .state
            .wrapping_mul(1_664_525)
            .wrapping_add(1_013_904_223);
        low + u64::from(self.state >> 8) % span / 4 * 4
    }

    fn place(
        &mut self,
        object: &str,
        (kind, region, group): (&str, usize, &'static str),
        inputs: &[(String, u64)],
    ) {
        let size: u64 = inputs.iter().map(|(_, size)| size).sum();
        let at = self.outputs.len() + 1;
        let name = format!(".{kind}.{object} ({at})");
        let address = REGIONS[region].1 + self.cursors[region];
        let mut offset = 0;
        for (input, part) in inputs {
            self.parts.push(Part {
                object: format!("obj/{object}.o"),
                input: format!("{input} ({at})"),
                size: *part,
                offset,
                output: name.clone(),
            });
            offset += part;
        }
        self.cursors[region] += size.div_ceil(16) * 16;
        self.used[region] += size;
        self.outputs.push(Output {
            name,
            region,
            address,
            size,
            group,
        });
    }

    fn module(&mut self, (object, prefix, functions): (&str, &str, [&str; 3])) {
        let sizes: Vec<u64> = functions.iter().map(|_| self.next(0x200, 0x8000)).collect();
        let inputs: Vec<(String, u64)> = functions
            .iter()
            .zip(&sizes)
            .map(|(name, size)| (format!(".text.{prefix}_{name}"), *size))
            .collect();
        let base = REGIONS[0].1 + self.cursors[0];
        let mut at = base;
        for (name, size) in functions.iter().zip(&sizes) {
            self.symbols.push(Symbol {
                name: format!("{prefix}_{name}"),
                address: at,
                size: *size,
            });
            at += size;
        }
        self.place(object, ("text", 0, "code"), &inputs);
        let constants = self.next(0x100, 0x4000);
        self.place(
            object,
            ("rodata", 0, "data"),
            &[(format!(".rodata.{prefix}"), constants)],
        );
        let variables = self.next(0x80, 0x1000);
        let zeroed = self.next(0x100, 0x2000);
        let data_at = REGIONS[2].1 + self.cursors[2];
        self.symbols.push(Symbol {
            name: format!("{prefix}_State"),
            address: data_at,
            size: variables,
        });
        self.place(
            object,
            ("data", 2, "data"),
            &[(format!(".data.{prefix}"), variables)],
        );
        self.place(
            object,
            ("bss", 2, "data"),
            &[(format!(".bss.{prefix}"), zeroed)],
        );
    }
}

fn linker_map() -> String {
    render(&built(), true)
}

fn unread_map() -> String {
    render(&built(), false)
}

fn built() -> Builder {
    let mut builder = Builder {
        state: 0x4D41_5031,
        cursors: [0; 4],
        used: [0; 4],
        outputs: Vec::new(),
        parts: Vec::new(),
        symbols: Vec::new(),
    };
    for module in MODULES {
        builder.module(module);
    }
    builder.place(
        "cal_table",
        ("rodata", 1, "data"),
        &[(".rodata.calibration".to_owned(), 0x3_800)],
    );
    builder.place(
        "stack",
        ("bss", 3, "data"),
        &[(".bss.stacks".to_owned(), 0xFF3C)],
    );
    builder
}

fn link_result(builder: &Builder) -> String {
    let mut text = String::from(
        "* Link Result\n| [in] | | | [out] | | |\n| File | Section | Size | Offset | Section | Size |\n",
    );
    let mut last = "";
    for part in &builder.parts {
        let size = builder
            .outputs
            .iter()
            .find(|out| out.name == part.output)
            .map_or(0, |out| out.size);
        let file = if part.object == last {
            ""
        } else {
            &part.object
        };
        last = &part.object;
        let _ = writeln!(
            text,
            "| {file} | {} | 0x{:X} | 0x{:X} | {} | 0x{size:X} |",
            part.input, part.size, part.offset, part.output
        );
    }
    text
}

fn locate_result(builder: &Builder) -> String {
    let mut text = format!("* Locate Result\n* Sections\n+ Space {SPACE} (MAU = 8bit)\n+------+\n");
    text.push_str("| Group | Chip | Section | Space addr | Chip addr | Size (MAU) | Alignment |\n|=====|=====|=====|=====|=====|=====|=====|\n");
    for out in &builder.outputs {
        let _ = writeln!(
            text,
            "| {} | {} | {} | 0x{:08X} | 0x0 | 0x{:X} | 0x10 |",
            out.group, REGIONS[out.region].0, out.name, out.address, out.size
        );
    }
    text
}

fn memory_usage(builder: &Builder) -> String {
    let mut text =
        String::from("* Memory Usage\n| Memory | Code | Data | Reserved | Free | Total |\n");
    for (at, (name, _, total)) in REGIONS.iter().enumerate() {
        let used = builder.used[at];
        let (code, data) = if at == 0 { (used, 0) } else { (0, used) };
        let _ = writeln!(
            text,
            "| {name} | {code} | {data} | {} | {} | {total} |",
            RESERVED[at],
            total - used - RESERVED[at]
        );
    }
    text
}

fn symbol_table(builder: &Builder, has_sizes: bool) -> String {
    let head = if has_sizes {
        "| Name | Address | Size | Space |"
    } else {
        "| Name | Address | Space |"
    };
    let mut text = format!("* Symbols (sorted on name)\n{head}\n");
    let mut symbols: Vec<&Symbol> = builder.symbols.iter().collect();
    symbols.sort_by(|a, b| a.name.cmp(&b.name));
    for symbol in symbols {
        let _ = if has_sizes {
            writeln!(
                text,
                "| {} | 0x{:08X} | 0x{:X} | {SPACE} |",
                symbol.name, symbol.address, symbol.size
            )
        } else {
            writeln!(
                text,
                "| {} | 0x{:08X} | {SPACE} |",
                symbol.name, symbol.address
            )
        };
    }
    text
}

const REMOVED: &str = "* Removed Sections\n| Section | File | Library | Symbol | Reason |\n| .text.CanDrv_Legacy (91) | obj/can_drv.o | | CanDrv_Legacy | unreferenced |\n";

const UNREAD: &str = "* Cross References\n======\ninvented reference lines\n* Call Graph\n======\ninvented call lines\n* Locate Rules\n======\ninvented rule lines\n* Future Extension\n======\ninvented extension lines\n";

fn render(builder: &Builder, has_sizes: bool) -> String {
    [
        "TASKING VX-toolset for TriCore linker map file\n* Tool and Invocation\nltc -o bcm_app.elf bcm_app.lsl\n".to_owned(),
        link_result(builder),
        locate_result(builder),
        memory_usage(builder),
        symbol_table(builder, has_sizes),
        REMOVED.to_owned(),
        if has_sizes { String::new() } else { UNREAD.to_owned() },
    ]
    .concat()
}
