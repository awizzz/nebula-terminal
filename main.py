#!/usr/bin/env python3
# Nebula Shell v0.1
# Windows shell frontend using only the Python standard library.

from __future__ import annotations

import ctypes
import json
import os
import re
import shutil
import subprocess
import sys
import time
from pathlib import Path

APP_NAME = "Nebula"
VERSION = "0.1"
HOME = Path.home()
DATA_DIR = HOME / ".nebula"
HISTORY_FILE = DATA_DIR / "history.json"
CONFIG_FILE = DATA_DIR / "config.json"

DEFAULT_CONFIG = {
    "show_banner": True,
    "show_git": True,
    "show_duration": True,
    "history_limit": 500,
    "theme": "hypr",
}

# ANSI / true-colour palette
RESET = "\x1b[0m"
BOLD = "\x1b[1m"
DIM = "\x1b[2m"
FG = "\x1b[38;2;205;214;244m"
MUTED = "\x1b[38;2;127;132;156m"
CYAN = "\x1b[38;2;137;220;235m"
BLUE = "\x1b[38;2;125;207;255m"
PURPLE = "\x1b[38;2;203;166;247m"
GREEN = "\x1b[38;2;158;206;106m"
YELLOW = "\x1b[38;2;229;192;123m"
RED = "\x1b[38;2;224;108;117m"

ANSI_RE = re.compile(r"\x1b\[[0-9;?]*[A-Za-z]")

def enable_ansi() -> None:
    """Enable VT sequences in the current Windows console when possible."""
    if os.name != "nt":
        return
    try:
        kernel32 = ctypes.windll.kernel32
        handle = kernel32.GetStdHandle(-11)  # STD_OUTPUT_HANDLE
        mode = ctypes.c_uint()
        if kernel32.GetConsoleMode(handle, ctypes.byref(mode)):
            kernel32.SetConsoleMode(handle, mode.value | 0x0004)
    except Exception:
        pass

def is_admin() -> bool:
    if os.name != "nt":
        return os.geteuid() == 0 if hasattr(os, "geteuid") else False
    try:
        return bool(ctypes.windll.shell32.IsUserAnAdmin())
    except Exception:
        return False

def set_title(title: str) -> None:
    sys.stdout.write(f"\x1b]0;{title}\x07")
    sys.stdout.flush()

def load_config() -> dict:
    DATA_DIR.mkdir(exist_ok=True)
    if not CONFIG_FILE.exists():
        CONFIG_FILE.write_text(json.dumps(DEFAULT_CONFIG, indent=2), encoding="utf-8")
        return DEFAULT_CONFIG.copy()
    try:
        data = json.loads(CONFIG_FILE.read_text(encoding="utf-8"))
        cfg = DEFAULT_CONFIG.copy()
        cfg.update(data)
        return cfg
    except Exception:
        return DEFAULT_CONFIG.copy()

def load_history(limit: int) -> list[str]:
    try:
        data = json.loads(HISTORY_FILE.read_text(encoding="utf-8"))
        if isinstance(data, list):
            return [str(x) for x in data][-limit:]
    except Exception:
        pass
    return []

def save_history(history: list[str], limit: int) -> None:
    try:
        DATA_DIR.mkdir(exist_ok=True)
        HISTORY_FILE.write_text(
            json.dumps(history[-limit:], ensure_ascii=False, indent=2),
            encoding="utf-8",
        )
    except Exception:
        pass

def compact_path(path: Path) -> str:
    try:
        resolved = path.resolve()
        home = HOME.resolve()
        if resolved == home:
            return "~"
        try:
            rel = resolved.relative_to(home)
            return "~/" + str(rel).replace("\\", "/")
        except ValueError:
            return str(resolved).replace("\\", "/")
    except Exception:
        return str(path).replace("\\", "/")

def git_branch(cwd: Path) -> str | None:
    try:
        p = subprocess.run(
            ["git", "branch", "--show-current"],
            cwd=str(cwd),
            stdout=subprocess.PIPE,
            stderr=subprocess.DEVNULL,
            text=True,
            timeout=0.4,
            creationflags=subprocess.CREATE_NO_WINDOW if os.name == "nt" else 0,
        )
        branch = p.stdout.strip()
        return branch or None
    except Exception:
        return None

def format_duration(seconds: float) -> str:
    ms = seconds * 1000
    if ms < 1000:
        return f"{ms:.0f}ms"
    if seconds < 60:
        return f"{seconds:.2f}s"
    return f"{seconds / 60:.1f}m"

def prompt(cwd: Path, last_code: int, last_duration: float, cfg: dict) -> str:
    admin = is_admin()
    status = f"{RED}{BOLD}ADMIN{RESET}" if admin else f"{GREEN}USER{RESET}"
    path_text = compact_path(cwd)

    extras = []
    if cfg.get("show_git", True):
        branch = git_branch(cwd)
        if branch:
            extras.append(f"{PURPLE}git:{branch}{RESET}")

    if cfg.get("show_duration", True) and last_duration > 0.01:
        extras.append(f"{MUTED}{format_duration(last_duration)}{RESET}")

    if last_code != 0:
        extras.append(f"{RED}exit:{last_code}{RESET}")

    suffix = ("  " + "  ".join(extras)) if extras else ""
    return (
        f"{MUTED}╭─{RESET} {status} "
        f"{CYAN}{path_text}{RESET}{suffix}\n"
        f"{MUTED}╰─{RESET}{BLUE}{BOLD}❯{RESET} "
    )

def banner() -> None:
    admin = is_admin()
    badge = f"{RED}{BOLD}ADMINISTRATOR{RESET}" if admin else f"{GREEN}STANDARD USER{RESET}"
    print()
    print(f"  {BOLD}{CYAN}NEBULA{RESET}  {MUTED}shell {VERSION}{RESET}")
    print(f"  {MUTED}Windows command frontend · {RESET}{badge}")
    print(f"  {MUTED}Type {RESET}{BOLD}help{RESET}{MUTED} for Nebula commands. Everything else goes to cmd.exe.{RESET}")
    print()

def elevate_current_shell() -> bool:
    """Relaunch Nebula through UAC. Returns True when a child was started."""
    if os.name != "nt":
        print(f"{YELLOW}Elevation is only implemented for Windows.{RESET}")
        return False

    if is_admin():
        print(f"{YELLOW}Nebula is already running as administrator.{RESET}")
        return False

    cwd = os.getcwd()

    if getattr(sys, "frozen", False):
        executable = sys.executable
        params = "--admin-child"
    else:
        executable = sys.executable
        script = str(Path(__file__).resolve())
        params = f'"{script}" --admin-child'

    try:
        result = ctypes.windll.shell32.ShellExecuteW(
            None,
            "runas",
            executable,
            params,
            cwd,
            1,
        )
        return result > 32
    except Exception as exc:
        print(f"{RED}Could not request elevation: {exc}{RESET}")
        return False

def run_as_admin_once(command: str, cwd: Path) -> int:
    """Run one command in an elevated cmd.exe window."""
    if os.name != "nt":
        print(f"{YELLOW}This command is Windows-only.{RESET}")
        return 1

    comspec = os.environ.get("COMSPEC", r"C:\Windows\System32\cmd.exe")
    # /k leaves the elevated window open so output is visible.
    params = f'/d /k "cd /d "{cwd}" && {command}"'
    try:
        result = ctypes.windll.shell32.ShellExecuteW(
            None, "runas", comspec, params, str(cwd), 1
        )
        return 0 if result > 32 else 1
    except Exception as exc:
        print(f"{RED}Could not launch elevated command: {exc}{RESET}")
        return 1

def resolve_cd_target(raw: str, cwd: Path) -> Path:
    raw = raw.strip()
    if not raw:
        return HOME

    # CMD accepts: cd /d D:\folder
    if raw.lower().startswith("/d "):
        raw = raw[3:].strip()

    if len(raw) >= 2 and raw[0] == raw[-1] == '"':
        raw = raw[1:-1]

    raw = os.path.expandvars(os.path.expanduser(raw))
    target = Path(raw)
    if not target.is_absolute():
        target = cwd / target
    return target.resolve()

def nebula_help() -> None:
    print(f"""
{BOLD}{CYAN}Nebula commands{RESET}

  {BOLD}admin{RESET}
      Relaunch Nebula with a UAC prompt. The elevated shell clearly shows ADMIN.

  {BOLD}sudo <command>{RESET}
      Run one command elevated in a separate cmd.exe window.
      Example: sudo net session

  {BOLD}cd <path>{RESET} / {BOLD}cd /d <path>{RESET}
      Change Nebula's persistent working directory.

  {BOLD}pushd <path>{RESET} / {BOLD}popd{RESET}
      Directory stack, persistent inside Nebula.

  {BOLD}set NAME=value{RESET}
      Set an environment variable for this Nebula session.

  {BOLD}pwd{RESET}
      Print the current directory.

  {BOLD}clear{RESET} / {BOLD}cls{RESET}
      Clear the terminal.

  {BOLD}history{RESET}
      Show Nebula command history.

  {BOLD}config{RESET}
      Open ~/.nebula/config.json in Notepad.

  {BOLD}reload{RESET}
      Reload config.json without restarting.

  {BOLD}exit{RESET}
      Close Nebula.

{MUTED}All other input is passed unchanged to cmd.exe /d /s /c.
That means normal CMD syntax works: pipes, redirections, &&, ||, .bat/.cmd,
dir, ipconfig, ping, git, python, powershell, ssh, winget, sc, netsh, etc.{RESET}
""".strip())

def execute_cmd(line: str, cwd: Path) -> int:
    comspec = os.environ.get("COMSPEC", "cmd.exe")
    try:
        completed = subprocess.run(
            [comspec, "/d", "/s", "/c", line],
            cwd=str(cwd),
            env=os.environ.copy(),
        )
        return int(completed.returncode)
    except FileNotFoundError:
        print(f"{RED}cmd.exe could not be found.{RESET}")
        return 9009
    except KeyboardInterrupt:
        print()
        return 130
    except Exception as exc:
        print(f"{RED}{exc}{RESET}")
        return 1

def main() -> int:
    enable_ansi()
    cfg = load_config()
    history = load_history(int(cfg.get("history_limit", 500)))
    dir_stack: list[Path] = []
    cwd = Path.cwd().resolve()

    # Optional start mode:
    #   python NebulaShell.py --admin
    if "--admin" in sys.argv and not is_admin():
        if elevate_current_shell():
            return 0
        return 1

    set_title(f"{APP_NAME} {'[ADMIN]' if is_admin() else ''}".strip())

    if cfg.get("show_banner", True):
        banner()

    last_code = 0
    last_duration = 0.0

    while True:
        try:
            line = input(prompt(cwd, last_code, last_duration, cfg)).strip()
        except EOFError:
            print()
            break
        except KeyboardInterrupt:
            print()
            last_code = 130
            last_duration = 0.0
            continue

        if not line:
            continue

        history.append(line)
        save_history(history, int(cfg.get("history_limit", 500)))

        raw = line
        cmd, _, rest = raw.partition(" ")
        lower = cmd.lower()
        started = time.perf_counter()

        try:
            if lower in {"exit", "quit"} and not rest:
                break

            elif lower in {"cls", "clear"} and not rest:
                os.system("cls" if os.name == "nt" else "clear")
                last_code = 0

            elif lower == "help" and not rest:
                nebula_help()
                last_code = 0

            elif lower == "pwd" and not rest:
                print(cwd)
                last_code = 0

            elif lower in {"cd", "chdir"}:
                try:
                    target = resolve_cd_target(rest, cwd)
                    if not target.exists():
                        print(f"{RED}The system cannot find the path specified.{RESET}")
                        last_code = 1
                    elif not target.is_dir():
                        print(f"{RED}Not a directory: {target}{RESET}")
                        last_code = 1
                    else:
                        cwd = target
                        os.chdir(cwd)
                        last_code = 0
                except Exception as exc:
                    print(f"{RED}{exc}{RESET}")
                    last_code = 1

            elif lower == "pushd":
                try:
                    target = resolve_cd_target(rest, cwd)
                    if not target.is_dir():
                        raise NotADirectoryError(target)
                    dir_stack.append(cwd)
                    cwd = target
                    os.chdir(cwd)
                    last_code = 0
                except Exception as exc:
                    print(f"{RED}{exc}{RESET}")
                    last_code = 1

            elif lower == "popd" and not rest:
                if not dir_stack:
                    print(f"{YELLOW}Directory stack is empty.{RESET}")
                    last_code = 1
                else:
                    cwd = dir_stack.pop()
                    os.chdir(cwd)
                    last_code = 0

            elif lower == "set":
                if not rest:
                    for key in sorted(os.environ):
                        print(f"{key}={os.environ[key]}")
                    last_code = 0
                elif "=" in rest:
                    name, value = rest.split("=", 1)
                    name = name.strip()
                    if not name:
                        print(f"{RED}Invalid variable name.{RESET}")
                        last_code = 1
                    elif value == "":
                        os.environ.pop(name, None)
                        last_code = 0
                    else:
                        os.environ[name] = value
                        last_code = 0
                else:
                    needle = rest.strip().lower()
                    matches = [
                        f"{k}={v}" for k, v in os.environ.items()
                        if k.lower().startswith(needle)
                    ]
                    if matches:
                        print("\n".join(sorted(matches)))
                        last_code = 0
                    else:
                        print(f"{RED}Environment variable not defined.{RESET}")
                        last_code = 1

            elif lower == "history" and not rest:
                start = max(0, len(history) - 50)
                for i, item in enumerate(history[start:], start=start + 1):
                    print(f"{MUTED}{i:>4}{RESET}  {item}")
                last_code = 0

            elif lower == "config" and not rest:
                if os.name == "nt":
                    subprocess.Popen(["notepad.exe", str(CONFIG_FILE)])
                    last_code = 0
                else:
                    print(CONFIG_FILE)
                    last_code = 0

            elif lower == "reload" and not rest:
                cfg = load_config()
                print(f"{GREEN}Configuration reloaded.{RESET}")
                last_code = 0

            elif lower == "admin" and not rest:
                if elevate_current_shell():
                    print(f"{GREEN}Elevated Nebula launched.{RESET}")
                    break
                last_code = 1

            elif lower == "sudo":
                if not rest.strip():
                    print(f"{YELLOW}Usage: sudo <command>{RESET}")
                    last_code = 1
                else:
                    last_code = run_as_admin_once(rest.strip(), cwd)

            else:
                # The important bit: let the real Windows command processor
                # parse and execute the command exactly like CMD would.
                last_code = execute_cmd(raw, cwd)

        finally:
            last_duration = time.perf_counter() - started

    save_history(history, int(cfg.get("history_limit", 500)))
    print(f"{MUTED}Nebula closed.{RESET}")
    return 0

if __name__ == "__main__":
    raise SystemExit(main())
