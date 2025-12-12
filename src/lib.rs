use std::fs::OpenOptions;
use std::io::Write;
use std::ptr::read_unaligned;
use std::time::Duration;

use windows::Win32::System::ProcessStatus::*;
use windows::Win32::System::Threading::*;

use eldenring::cs::GameDataMan;
use eldenring::{
    cs::{CSTaskGroupIndex, CSTaskImp, WorldChrMan},
    fd4::FD4TaskData,
    util::{input, system::wait_for_system_init},
};
use fromsoftware_shared::{program::Program, task::*, FromStatic};
use windows::core::PCSTR;
use windows::Win32::System::LibraryLoader::GetModuleHandleA;

use pelite::pattern;
use pelite::pe64::Pe;
use pelite::pe64::PeView;

const SP_EFFECT: i32 = 4330;

// TODO: will crash on Error
fn log(s: &str) {
    let now = chrono::Local::now()
        .format("%y.%m.%d | %H:%M:%S.%3f")
        .to_string();
    let _ = OpenOptions::new()
        .create(true)
        .append(true)
        .open(r"C:\Program Files (x86)\Steam\steamapps\common\ELDEN RING\Game\mod_dll\apply-speffect\logs.log")
        .map(|mut f| writeln!(f, "[{}]: {}", now, s))
        .unwrap();
}

fn get_module_info() -> (*const u8, usize) {
    unsafe {
        let hmod = match GetModuleHandleA(PCSTR(std::ptr::null())) {
            Ok(hmod) => {
                log("GetModuleHandleA succeeded");
                hmod
            }
            Err(_) => {
                log("GetModuleHandleA failed");
                panic!()
            }
        };
        let mut info = MODULEINFO::default();

        let module_info = GetModuleInformation(
            GetCurrentProcess(),
            hmod,
            &mut info,
            size_of::<MODULEINFO>() as u32,
        );

        match module_info {
            Ok(_) => log("GetModuleInformation succeeded"),
            Err(_) => log("GetModuleInformation failed"),
        };

        let base = info.lpBaseOfDll as *const u8;
        let size = info.SizeOfImage as usize;

        log(&format!("base VA {}; size {}", *base, size));

        (base, size)
    }
}

fn get_pe_view() -> PeView<'static> {
    let (base, size) = get_module_info();
    unsafe {
        let slice = std::slice::from_raw_parts(base, size);
        match PeView::from_bytes(slice) {
            Ok(v) => {
                log("PeView created");
                v
            }
            Err(e) => {
                log(&format!("Error while creating PeView {}", e.to_string()));
                panic!()
            }
        }
    }
}

fn get_game_data_man() -> &'static mut GameDataMan {
    let offset = 3;
    let additional = 7;
    let pattern_str = "48 8B 05 ? ? ? ? 48 85 C0 74 05 48 8B 40 58 C3 C3";

    let pattern = match pattern::parse(pattern_str) {
        Ok(p) => p,
        Err(e) => {
            log(&format!("pattern parse error: {}", e.to_string()));
            panic!()
        }
    };

    let pe = get_pe_view();
    let text_header = match pe
        .section_headers()
        .iter()
        .find(|h| &h.Name[..8] == b".text\0\0\0")
    {
        Some(h) => {
            log("Text header found");
            h
        }
        None => {
            log("Text header not found");
            panic!()
        }
    };

    let scanner = pe.scanner();

    let mut rva = [0; 8];
    let mut matches = scanner.matches(&*pattern, text_header.file_range());

    let (base, _) = get_module_info();

    let game_data_man = loop {
        if !matches.next(&mut rva) {
            log(&format!("No RVA found for pattern {}", pattern_str));
            panic!()
        }

        let rva = rva[0] as usize;
        log(&format!("Found RVA {:?}", rva));

        let resolved_va = unsafe {
            let aob_va = base.add(rva);
            log(&format!("AoB VA [{:?}]", aob_va));

            let offset_value = read_unaligned(aob_va.add(offset) as *const i32);
            log(&format!("Offset value [{:?}]", offset_value));

            let resolved_va = aob_va.add(additional).offset(offset_value as isize);
            log(&format!("Resolved VA [{:?}]", resolved_va));

            resolved_va
        };

        let pointer: *const *mut GameDataMan = resolved_va as *const *mut GameDataMan;
        let game_data_man_ptr: *mut GameDataMan = unsafe { *pointer };
        break unsafe { &mut *game_data_man_ptr };
    };
    game_data_man
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn DllMain(_hmodule: u64, reason: u32) -> bool {
    if reason != 1 {
        return true;
    }

    std::thread::spawn(move || {
        wait_for_system_init(&Program::current(), Duration::MAX)
            .expect("Timeout waiting for system init");

        let game_data_man = get_game_data_man();

        let cs_task = unsafe { CSTaskImp::instance().unwrap() };
        cs_task.run_recurring(
            |_: &FD4TaskData| {
                let Ok(world_chr_man) = (unsafe { WorldChrMan::instance() }) else {
                    return;
                };
                let Some(ref mut main_player) = world_chr_man.main_player else {
                    return;
                };

                if input::is_key_pressed(0x4F) {
                    log("o pressed");
                    main_player.chr_ins.apply_speffect(SP_EFFECT, true);
                    log(&format!("deaths: {}", game_data_man.death_count))
                }

                if input::is_key_pressed(0x50) {
                    log("p pressed");
                    main_player.chr_ins.remove_speffect(SP_EFFECT);
                }
            },
            CSTaskGroupIndex::FrameBegin,
        );
    });
    true
}
