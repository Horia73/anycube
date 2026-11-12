//! `anycube app`: the interface, on this machine.
//!
//! The page in `app/` (a cube to scramble, paint, show to a camera, turn by
//! hand and solve) is built into the program, served on 127.0.0.1 by
//! `server`, and opened in a window of its own by `window`. `install` puts
//! an entry with an icon among the computer's applications.

pub mod assets;
pub mod install;
pub mod server;
pub mod window;

use std::path::PathBuf;

pub const HELP: &str = "\
anycube app                 open the interface in a window of its own
anycube app --browser       open it in the default browser instead
anycube app --no-window     only serve it, and print its address
anycube app --port N        serve on port N (any free port otherwise)
anycube app --install       add anycube to this computer's applications, with its icon
anycube app --uninstall     take that entry away again

Lookup tables are kept in ANYCUBE_TABLES if it is set, else in ./tables if
that directory exists, else with the user's application data (where the
installed program's command line keeps them too). The first solve of a size
builds its tables; the interface shows the progress.
";

/// The largest cube the interface offers: what its 3D view draws at speed.
/// The command line solves larger ones.
const MAX_SIZE: usize = 50;

/// Where this user's application data goes.
pub fn data_dir() -> PathBuf {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));
    if cfg!(target_os = "macos") {
        home.join("Library/Application Support/anycube")
    } else {
        std::env::var_os("XDG_DATA_HOME").map(PathBuf::from).unwrap_or_else(|| home.join(".local/share")).join("anycube")
    }
}

/// Where the tables are kept when the interface runs: where the command
/// line would keep them from here, if that exists, else with the user's
/// application data, so that an installed application has one place for
/// them wherever it is started from.
pub fn tables_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("ANYCUBE_TABLES").or_else(|| std::env::var_os("CUBE444_TABLES")) {
        return PathBuf::from(dir);
    }
    let here = PathBuf::from("tables");
    if here.is_dir() {
        return here;
    }
    data_dir().join("tables")
}

/// For the command line of an installed program: it keeps its tables where
/// the interface does, with the user's application data, and not in a
/// `tables` directory wherever it happens to be run. A program run from the
/// repository it was built in (`target/...`) is left as it was, and so is
/// one run where a `tables` directory already is, or told where by
/// `ANYCUBE_TABLES`.
pub fn settle_tables() {
    if std::env::var_os("ANYCUBE_TABLES").is_some() || std::env::var_os("CUBE444_TABLES").is_some() {
        return;
    }
    if std::path::Path::new("tables").is_dir() {
        return;
    }
    let built_here = std::env::current_exe().is_ok_and(|exe| exe.components().any(|part| part.as_os_str() == "target"));
    if !built_here {
        std::env::set_var("ANYCUBE_TABLES", data_dir().join("tables"));
    }
}

/// The process that started this one.
#[cfg(unix)]
fn parent() -> i32 {
    extern "C" {
        fn getppid() -> i32;
    }
    unsafe { getppid() }
}

#[cfg(not(unix))]
fn parent() -> i32 {
    0
}

fn fail(message: &str) -> ! {
    eprintln!("error: {}", message);
    std::process::exit(1)
}

pub fn main(args: &[String]) {
    let has = |flag: &str| args.iter().any(|a| a == flag);
    if has("--worker") {
        return server::worker();
    }
    if has("--help") || has("-h") {
        print!("{}", HELP);
        return;
    }
    if has("--install") {
        let binary = std::env::current_exe().unwrap_or_else(|e| fail(&format!("cannot tell where this program is: {}", e)));
        match install::add(&binary) {
            Ok(places) => {
                println!("anycube is among your applications now. Written:");
                for p in places {
                    println!("  {}", p.display());
                }
                println!("It starts {}; if that program is moved, run this again.", binary.display());
                println!("To take it away: anycube app --uninstall");
            }
            Err(e) => fail(&format!("could not add the application entry: {}", e)),
        }
        return;
    }
    if has("--uninstall") {
        match install::remove() {
            Ok(gone) if gone.is_empty() => println!("There was no application entry to take away."),
            Ok(gone) => {
                println!("Removed:");
                for p in gone {
                    println!("  {}", p.display());
                }
            }
            Err(e) => fail(&format!("could not remove the application entry: {}", e)),
        }
        return;
    }
    if let Some(unknown) = args.iter().enumerate().find(|(i, a)| {
        !matches!(a.as_str(), "--browser" | "--no-window" | "--with-parent" | "--port") && !(*i > 0 && args[i - 1] == "--port")
    }) {
        eprintln!("unknown argument '{}'\n\n{}", unknown.1, HELP);
        std::process::exit(2);
    }
    let port = match args.iter().position(|a| a == "--port") {
        None => 0,
        Some(i) => args.get(i + 1).and_then(|p| p.parse::<u16>().ok()).unwrap_or_else(|| fail("--port takes a number from 1 to 65535")),
    };

    // The solvers (in the worker this program starts) read the variable.
    let tables = tables_dir();
    if std::fs::create_dir_all(&tables).is_err() {
        fail(&format!("cannot create the tables directory {}", tables.display()));
    }
    std::env::set_var("ANYCUBE_TABLES", &tables);

    let running = server::start(port, MAX_SIZE).unwrap_or_else(|e| fail(&e));
    // The address first and alone on its line: whoever started this program reads it there.
    println!("{}", running.url);
    eprintln!("anycube is at {} (tables in {}). Ctrl-C stops it.", running.url, tables.display());

    if has("--no-window") {
        // Started by a window of its own (the application entry, which
        // says `--with-parent`), this ends when the window's process does:
        // a process whose parent has ended is handed to another. Started
        // by hand, it runs until it is stopped.
        let started_by = parent();
        loop {
            std::thread::sleep(std::time::Duration::from_secs(1));
            if has("--with-parent") && parent() != started_by {
                return;
            }
        }
    }
    let opened = if has("--browser") {
        window::open_tab(&running.url)
    } else {
        window::open(&running.url, &data_dir())
    };
    match opened {
        window::Opened::Window(mut child) => {
            // The window is the application: when it closes, so does this.
            let _ = child.wait();
            window::tidy();
        }
        window::Opened::Tab => loop {
            std::thread::park();
        },
        window::Opened::Nothing => {
            eprintln!("No window or browser could be opened here; open {} yourself.", running.url);
            loop {
                std::thread::park();
            }
        }
    }
}
