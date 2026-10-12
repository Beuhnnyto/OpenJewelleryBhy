//! OpenJewelleryBhy installer for Windows.
//!
//! Installs per user into `%LOCALAPPDATA%\Programs\OpenJewelleryBhy` (no
//! administrator rights needed), creates Start menu and Desktop shortcuts,
//! associates `.ojb` files and registers an uninstaller in "Apps & features".
//! Run with `/S` for a silent install; the installed `uninstall.exe` removes
//! everything.
#![cfg_attr(windows, windows_subsystem = "windows")]

include!(concat!(env!("OUT_DIR"), "/payload.rs"));

const APP: &str = "OpenJewelleryBhy";
const VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg(not(windows))]
fn main() {
    eprintln!("{APP} {VERSION} setup is a Windows installer ({} bytes of payload).", PAYLOAD.len());
    eprintln!("On Linux, use the install.sh script from the Linux archive.");
}

#[cfg(windows)]
fn main() {
    win::run();
}

#[cfg(windows)]
mod win {
    use super::{APP, PAYLOAD, VERSION};
    use std::os::windows::process::CommandExt;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use windows_sys::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONERROR, MB_ICONINFORMATION, MB_ICONQUESTION, MB_YESNO};

    const NO_WINDOW: u32 = 0x0800_0000;
    const UNINSTALL_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\OpenJewelleryBhy";

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn msg(text: &str, flags: u32) -> i32 {
        let (t, c) = (wide(text), wide(&format!("{APP} {VERSION}")));
        unsafe { MessageBoxW(std::ptr::null_mut(), t.as_ptr(), c.as_ptr(), flags) }
    }

    fn install_dir() -> PathBuf {
        let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(r"C:\Users\Public"));
        base.join("Programs").join(APP)
    }

    fn reg(args: &[&str]) -> bool {
        Command::new("reg").args(args).creation_flags(NO_WINDOW).status().map(|s| s.success()).unwrap_or(false)
    }

    fn powershell(script: &str) -> bool {
        Command::new("powershell")
            .args(["-NoProfile", "-ExecutionPolicy", "Bypass", "-NonInteractive", "-Command", script])
            .creation_flags(NO_WINDOW)
            .status()
            .map(|s| s.success())
            .unwrap_or(false)
    }

    fn ps_quote(p: &Path) -> String {
        format!("'{}'", p.display().to_string().replace('\'', "''"))
    }

    pub fn run() {
        let args: Vec<String> = std::env::args().skip(1).map(|a| a.to_ascii_lowercase()).collect();
        let silent = args.iter().any(|a| a == "/s" || a == "--silent");
        let result = if args.iter().any(|a| a == "--uninstall" || a == "/u") { uninstall(silent) } else { install(silent) };
        if let Err(e) = result {
            if !silent {
                msg(&format!("Setup failed:\n\n{e}"), MB_ICONERROR);
            }
            std::process::exit(1);
        }
    }

    fn install(silent: bool) -> Result<(), String> {
        if PAYLOAD.is_empty() {
            return Err("this setup was built without its payload".into());
        }
        let dir = install_dir();
        if !silent
            && msg(
                &format!(
                    "Install {APP} {VERSION} into:\n{}\n\nStart menu and Desktop shortcuts will be created and .ojb designs will open with {APP}.\nNo administrator rights are needed.\n\nContinue?",
                    dir.display()
                ),
                MB_YESNO | MB_ICONQUESTION,
            ) != IDYES
        {
            return Ok(());
        }
        std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(PAYLOAD)).map_err(|e| e.to_string())?;
        for i in 0..zip.len() {
            let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
            let Some(rel) = f.enclosed_name() else { continue };
            let target = dir.join(rel);
            if f.is_dir() {
                std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
                continue;
            }
            if let Some(p) = target.parent() {
                std::fs::create_dir_all(p).map_err(|e| e.to_string())?;
            }
            let mut out = std::fs::File::create(&target)
                .map_err(|e| format!("cannot write {} (is {APP} still running?): {e}", target.display()))?;
            std::io::copy(&mut f, &mut out).map_err(|e| e.to_string())?;
        }
        let exe = dir.join("openjewellerybhy.exe");
        let ico = dir.join("ojb.ico");
        let uninstaller = dir.join("uninstall.exe");
        let me = std::env::current_exe().map_err(|e| e.to_string())?;
        std::fs::copy(&me, &uninstaller).map_err(|e| e.to_string())?;

        // shortcuts (Start menu + Desktop)
        let script = format!(
            "$w = New-Object -ComObject WScript.Shell; foreach ($f in @([Environment]::GetFolderPath('Programs'), [Environment]::GetFolderPath('Desktop'))) {{ \
             $s = $w.CreateShortcut((Join-Path $f '{APP}.lnk')); $s.TargetPath = {}; $s.WorkingDirectory = {}; \
             $s.IconLocation = {}; $s.Description = 'Parametric jewellery CAD'; $s.Save() }}",
            ps_quote(&exe),
            ps_quote(&dir),
            ps_quote(&ico)
        );
        powershell(&script);

        // .ojb file association
        let open_cmd = format!("\"{}\" \"%1\"", exe.display());
        let ico_s = ico.display().to_string();
        reg(&["add", r"HKCU\Software\Classes\.ojb", "/ve", "/d", "OpenJewelleryBhy.Design", "/f"]);
        reg(&["add", r"HKCU\Software\Classes\OpenJewelleryBhy.Design", "/ve", "/d", "OpenJewelleryBhy design", "/f"]);
        reg(&["add", r"HKCU\Software\Classes\OpenJewelleryBhy.Design\DefaultIcon", "/ve", "/d", &ico_s, "/f"]);
        reg(&["add", r"HKCU\Software\Classes\OpenJewelleryBhy.Design\shell\open\command", "/ve", "/d", &open_cmd, "/f"]);

        // Apps & features entry
        let un = format!("\"{}\" --uninstall", uninstaller.display());
        let size_kb = dir_size(&dir) / 1024;
        for (name, ty, val) in [
            ("DisplayName", "REG_SZ", APP.to_string()),
            ("DisplayVersion", "REG_SZ", VERSION.to_string()),
            ("Publisher", "REG_SZ", "OpenJewelleryBhy contributors".to_string()),
            ("DisplayIcon", "REG_SZ", ico_s.clone()),
            ("InstallLocation", "REG_SZ", dir.display().to_string()),
            ("UninstallString", "REG_SZ", un.clone()),
            ("QuietUninstallString", "REG_SZ", format!("{un} /S")),
            ("NoModify", "REG_DWORD", "1".to_string()),
            ("NoRepair", "REG_DWORD", "1".to_string()),
            ("EstimatedSize", "REG_DWORD", size_kb.to_string()),
        ] {
            reg(&["add", UNINSTALL_KEY, "/v", name, "/t", ty, "/d", &val, "/f"]);
        }
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED as i32, SHCNF_IDLIST, std::ptr::null(), std::ptr::null()) };

        if !silent && msg(&format!("{APP} {VERSION} is installed.\n\nLaunch it now?"), MB_YESNO | MB_ICONINFORMATION) == IDYES {
            let _ = Command::new(&exe).current_dir(&dir).spawn();
        }
        Ok(())
    }

    fn dir_size(p: &Path) -> u64 {
        std::fs::read_dir(p)
            .map(|rd| {
                rd.flatten()
                    .map(|e| match e.metadata() {
                        Ok(m) if m.is_dir() => dir_size(&e.path()),
                        Ok(m) => m.len(),
                        Err(_) => 0,
                    })
                    .sum()
            })
            .unwrap_or(0)
    }

    fn uninstall(silent: bool) -> Result<(), String> {
        let dir = install_dir();
        if !silent && msg(&format!("Remove {APP} from this computer?\n\nYour .ojb designs are not deleted."), MB_YESNO | MB_ICONQUESTION) != IDYES {
            return Ok(());
        }
        powershell(&format!(
            "foreach ($f in @([Environment]::GetFolderPath('Programs'), [Environment]::GetFolderPath('Desktop'))) {{ Remove-Item -LiteralPath (Join-Path $f '{APP}.lnk') -ErrorAction SilentlyContinue }}"
        ));
        reg(&["delete", UNINSTALL_KEY, "/f"]);
        reg(&["delete", r"HKCU\Software\Classes\OpenJewelleryBhy.Design", "/f"]);
        reg(&["delete", r"HKCU\Software\Classes\.ojb", "/f"]);
        unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED as i32, SHCNF_IDLIST, std::ptr::null(), std::ptr::null()) };
        // this uninstaller lives in the folder: delete it once we have exited
        let _ = Command::new("cmd")
            .args(["/C", &format!("ping 127.0.0.1 -n 3 > nul & rmdir /s /q \"{}\"", dir.display())])
            .creation_flags(NO_WINDOW)
            .spawn();
        if !silent {
            msg(&format!("{APP} has been removed."), MB_ICONINFORMATION);
        }
        Ok(())
    }
}
