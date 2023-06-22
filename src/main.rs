use std::sync::Arc;
use std::sync::Mutex;
use std::env::var;
use std::fs::remove_file;


use std::fs::File;
use std::io::{self, Write};
use daemonize::Daemonize;



use clap::{crate_version, load_yaml, App};
use swayipc::reply::Event::Window;
use swayipc::reply::WindowChange;
use swayipc::{Connection, EventType};

type Res<T> = std::result::Result<T, Box<dyn std::error::Error>>;

fn get_current_focused_id() -> Res<i64> {
    Connection::new()?
        .get_tree()?
        .find_focused_as_ref(|n| n.focused)
        .map(|n| n.id)
        .ok_or_else(|| Err("Failed to get current Focused ID").unwrap())
}

fn handle_signal(last_focused: &Arc<Mutex<Vec<i64>>>) -> Res<()> {
    let mut focused_ids = last_focused.lock().unwrap();
    let mut valid_id = None;

    for &id in focused_ids.iter().rev() {
        if Connection::new()?.run_command(format!("[con_id={}] focus", id)).is_ok() {
            valid_id = Some(id);
            break;
        }
    }

    if let Some(id) = valid_id {
        focused_ids.retain(|&x| x != id);
    }

    Ok(())
}


fn unbind_key() -> Res<()> {
    let yml = load_yaml!("args.yml");
    let args = App::from_yaml(yml).version(crate_version!()).get_matches();
    let key_combo = args.value_of("combo").unwrap_or("Mod1+Tab");

    let pid_file = format!(
        "{}/sway-alttab.pid",
        var("XDG_RUNTIME_DIR").unwrap_or("/tmp".to_string())
    );
    Connection::new()?.run_command(format!(
        "unbindsym {} exec pkill -USR1 -F {}",
        key_combo, pid_file
    ))?;
    Ok(())
}

fn bind_key() -> Res<()> {
    let yml = load_yaml!("args.yml");
    let args = App::from_yaml(yml).version(crate_version!()).get_matches();
    let key_combo = args.value_of("combo").unwrap_or("Mod1+Tab");

    let pid_file = format!(
        "{}/sway-alttab.pid",
        var("XDG_RUNTIME_DIR").unwrap_or("/tmp".to_string())
    );

    Connection::new()?.run_command(format!(
        "bindsym {} exec pkill -USR1 -F {}",
        key_combo, pid_file
    ))?;
    Ok(())
}

fn start_daemon() -> Res<()> {
    let dir = var("XDG_RUNTIME_DIR").unwrap_or("/tmp".to_string());
    let stdout_file = File::create("/dev/null")?;

    unsafe { signal_hook::register(signal_hook::SIGTERM, cleanup)? };
    unsafe { signal_hook::register(signal_hook::SIGINT, cleanup)? };

    if true {
        Ok(daemonize::Daemonize::new()
            .pid_file(format!("{}/sway-alttab.pid", dir))
            .chown_pid_file(true)
            .working_directory(dir)
            .stdout(stdout_file)
            .start()?)
    } else {
        Ok(())
    }

}

fn cleanup() {
    let dir = var("XDG_RUNTIME_DIR").unwrap_or("/tmp".to_string());
    remove_file(format!("{}/sway-alttab.pid", dir)).unwrap();
    unbind_key().unwrap();
    println!("Exiting sway-alttab");
}

fn main() -> Res<()> {
    let last_focus = Arc::new(Mutex::new(Vec::new()));
    let mut cur_focus = get_current_focused_id()?;
    let clone = Arc::clone(&last_focus);

    unsafe {
        signal_hook::register(signal_hook::SIGUSR1, move || {
            println!("ok");
            handle_signal(&clone).unwrap();
        })?
    };

    start_daemon()?;

    bind_key()?;

    let subs = [EventType::Window];
    let mut events = Connection::new()?.subscribe(&subs)?;

    loop {
        let event = events.next();
        if let Some(Ok(Window(ev))) = event {
            if ev.change == WindowChange::Focus {
                if cur_focus != ev.container.id {
                let mut last = last_focus.lock().unwrap();
                let last_clone = last.clone();
                let result = last_clone.iter().position(|&r| r == cur_focus);
                match result {
                    Some(index) => last.remove(index),
                    None => -1,
                };
                last.push(cur_focus);
                println!("cur_focus {} ev.container.id {}", cur_focus, ev.container.id);
                cur_focus = ev.container.id;
                println!("length {} top {} val {}", last.len(), last[0], cur_focus);
                }
            } else if ev.change == WindowChange::Close {
                let mut last = last_focus.lock().unwrap();
                let result = last.iter().position(|&r| r == ev.container.id);
                match result {
                    Some(index) => last.remove(index),
                    None => -1,
                };
                println!("deleting {}", ev.container.id);
                cur_focus = last[0];
            }
        } else {
            cleanup();
        }
    }
}

