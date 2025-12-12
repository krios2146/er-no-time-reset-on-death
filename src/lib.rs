use std::time::Duration;

use eldenring::{
    cs::{CSTaskGroupIndex, CSTaskImp, WorldAreaTime, WorldChrMan},
    fd4::FD4TaskData,
    util::system::wait_for_system_init,
};
use fromsoftware_shared::{program::Program, task::*, FromStatic};

fn get_world_area_time() -> &'static mut WorldAreaTime {
    unsafe { WorldAreaTime::instance() }.unwrap()
}

fn get_current_time() -> (u8, u8, u8) {
    let world_area_time = get_world_area_time();

    let hours = world_area_time.clock.date.hours();
    let minutes = world_area_time.clock.date.minutes();
    let seconds = world_area_time.clock.date.seconds();

    (hours, minutes, seconds)
}

fn set_current_time(hours: u8, minutes: u8, seconds: u8) {
    let world_area_time = get_world_area_time();

    world_area_time.request_time(hours as u32, minutes as u32, seconds as u32);
}

#[unsafe(no_mangle)]
pub unsafe extern "C" fn DllMain(_hmodule: u64, reason: u32) -> bool {
    if reason != 1 {
        return true;
    }

    std::thread::spawn(move || {
        wait_for_system_init(&Program::current(), Duration::MAX)
            .expect("Timeout waiting for system init");

        let mut is_dead = false;
        let mut time_of_death: (u8, u8, u8) = (0, 0, 0);

        let cs_task = unsafe { CSTaskImp::instance().unwrap() };

        cs_task.run_recurring(
            move |_: &FD4TaskData| {
                let Ok(world_chr_man) = (unsafe { WorldChrMan::instance() }) else {
                    return;
                };
                let Some(ref mut main_player) = world_chr_man.main_player else {
                    return;
                };

                let player_hp = main_player.chr_ins.module_container.data.hp;

                if player_hp <= 0 && !is_dead {
                    is_dead = true;
                    time_of_death = get_current_time();
                }

                if player_hp > 0 && is_dead {
                    is_dead = false;
                    set_current_time(time_of_death.0, time_of_death.1, time_of_death.2);
                }
            },
            CSTaskGroupIndex::FrameBegin,
        );
    });
    true
}
