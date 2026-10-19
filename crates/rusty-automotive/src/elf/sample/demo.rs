use ::object::elf;

use super::{Named, Part, Spec, bss, build, data, func, line_program, object, text};

const DEMO_FUNCTIONS: [&str; 24] = [
    "Reset_Handler",
    "SystemInit",
    "main",
    "Can_Init",
    "Can_MainFunction_Read",
    "Can_MainFunction_Write",
    "CanIf_RxIndication",
    "Com_MainFunctionRx",
    "Com_MainFunctionTx",
    "Dcm_MainFunction",
    "Dem_SetEventStatus",
    "EcuM_Init",
    "Fee_MainFunction",
    "Pwm_Init",
    "Os_Task_1ms",
    "Os_StartScheduler",
    "Pwm_SetDutyCycle",
    "Rte_Read_DoorSwitch",
    "Rte_Write_InteriorLight",
    "WdgM_MainFunction",
    "_ZN3bcm6Lights6updateEv",
    "_ZN3bcm5Doors4scanEj",
    "HardFault_Handler",
    "SysTick_Handler",
];

const DEMO_OBJECTS: [(&str, u64, usize); 8] = [
    ("Can_ControllerState", 8, 1),
    ("Com_RxBuffer", 512, 2),
    ("Com_TxBuffer", 256, 2),
    ("Dem_EventMemory", 1024, 2),
    ("EcuM_State", 4, 1),
    ("g_tick_count", 4, 1),
    ("Lin_ScheduleTable", 96, 1),
    ("Os_Stack", 2048, 2),
];

pub fn demo() -> Result<Vec<u8>, String> {
    let base = 0x0000_8000;
    let mut symbols: Vec<Named> = DEMO_FUNCTIONS
        .iter()
        .enumerate()
        .map(|(at, name)| {
            let size = 0x40 + (at as u64 % 5) * 0x30;
            func(name, base + at as u64 * 0x100 + 1, size, 0)
        })
        .collect();
    let mut cursor = [0u64; 3];
    for (name, size, part) in DEMO_OBJECTS {
        let address = if part == 1 { 0x2000_0000 } else { 0x2000_0400 } + cursor[part];
        cursor[part] += size;
        let mut symbol = object(name, address, size, part);
        if name.starts_with("g_") {
            symbol.bind = elf::STB_LOCAL;
        }
        symbols.push(symbol);
    }
    let rows: Vec<(u64, u64, u64)> = (0..DEMO_FUNCTIONS.len() as u64)
        .filter(|at| at % 4 != 3)
        .map(|at| (base + at * 0x100, 0x40, 20 + at * 37))
        .collect();
    let spec = Spec {
        is_64: false,
        is_big: false,
        machine: elf::EM_ARM,
        entry: base + 1,
        parts: vec![
            text(base, 0x1800),
            Part {
                load_address: base + 0x1800,
                ..data(0x2000_0000, 0x400)
            },
            bss(0x2000_0400, 0x1000),
        ],
        symbols: Some(symbols),
        loads: vec![
            (0, elf::PF_R | elf::PF_X),
            (1, elf::PF_R | elf::PF_W),
            (2, elf::PF_R | elf::PF_W),
        ],
        debug: line_program(false, false, &rows)?,
    };
    build(&spec)
}
