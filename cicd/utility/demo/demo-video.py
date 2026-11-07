#!/usr/bin/env python3

##	Purpose:
##		Records tukzmon's demo video and README gif, per script.txt beside this.
##		A real xfce4-panel with the release plugin runs on a private Xvfb (never
##		:0), under xfwm4, with a throwaway HOME. The mouse is driven through
##		XTEST, clicks get foley synced to the input times, and captions sit in a
##		band under the desktop. 2 takes from one scene list:
##		  video: 1920x1080 at 60 fps, h265, GDK_SCALE=2 and QT_SCALE_FACTOR=2, with audio
##		  gif:   960x540 at 50 fps, scale 1, silent
##		Both lay out the same, so scene coordinates are written once, in gif px.
##		The long-run scene gets its days of history from
##		core/examples/demo-history.rs, written while the panel is down.
##	Syntax:
##		demo-video.py [--profile video,gif] [--scenes a,b,...] [--display :89]
##		              [--lib-dir DIR] [--history-bin FILE] [--keep-work]
##		              [--no-rotate] [--no-asset] [--out-dir DIR] [--shot FILE]
##		--lib-dir has libtukzmon.so and tukzmon-settings, target/release by
##		default. --shot runs the scenes named with no capture, then writes one
##		screenshot, which is how the coordinates below were measured.
##	Notes:
##		AV sync: the root is flashed white once frames flow and again at the end,
##		at wall-clock times that are kept. The 2 bright frames in the capture
##		anchor every event, and the capture's rate between them.
##		Started from nemo-anywhere's recorder (cicd/utility/demo-video/).
##		Sounds and their licenses are in ./sounds/.
##		Needs python3-xlib and numpy.
##	History: at bottom.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT

import argparse
import contextlib
import enum
import itertools
import os
import random
import re
import select
import shutil
import signal
import subprocess
import sys
import time
import wave
from collections.abc import Callable, Iterable
from dataclasses import dataclass, field
from pathlib import Path
from types import TracebackType
from typing import IO

import numpy as np
from Xlib import X
from Xlib import display as xdisplay

ME_DIR = Path(__file__).resolve().parent
REPO = ME_DIR.parents[2]  # repo/cicd/utility/demo -> repo
OUT_DIR = REPO.parent / "private" / "demo"  # beside the repo, not in it
SOUNDS = ME_DIR / "sounds"

SR = 48000
BANNER_TTF = "/usr/share/fonts/truetype/lato/Lato-Semibold.ttf"
BANNER_FG = "0xFFD866"
LEAD_S = 0.6
TAIL_HOLD_S = 2.0
# a gif fade is a new full frame each step, so it cuts to black instead
GIF_TAIL_BLACK_S = 3.0
VIDEO_FADE_IN_S = 0.5
VIDEO_TAIL_BLACK_S = 0.5
FOLEY_LAG = 0.01

# gif px; the video doubles all of it
VIEW_W, VIEW_H = 960, 540
BAND = 46  # captions, at the bottom
PANEL = 56  # panel height
DESK = "#1d2430"  # root color, the band's too

# a panel like a real one, with tukzmon at the right among the usual items.
# tukzmon stays plugin-1, which names its history file.
PANEL_ORDER = (2, 3, 4, 1, 5, 6, 7, 8, 9, 11, 10)
PANEL_PLUGINS = """\
    <property name="plugin-1" type="string" value="tukzmon"/>
    <property name="plugin-2" type="string" value="applicationsmenu">
      <property name="show-button-title" type="bool" value="false"/>
    </property>
    <property name="plugin-3" type="string" value="tasklist">
      <property name="grouping" type="uint" value="0"/>
      <property name="show-handle" type="bool" value="false"/>
    </property>
    <property name="plugin-4" type="string" value="separator">
      <property name="expand" type="bool" value="true"/>
      <property name="style" type="uint" value="0"/>
    </property>
    <property name="plugin-5" type="string" value="separator">
      <property name="style" type="uint" value="0"/>
    </property>
    <property name="plugin-6" type="string" value="systray">
      <property name="square-icons" type="bool" value="false"/>
      <property name="single-row" type="bool" value="true"/>
      <property name="icon-size" type="uint" value="24"/>
    </property>
    <property name="plugin-7" type="string" value="notification-plugin"/>
    <property name="plugin-8" type="string" value="separator">
      <property name="style" type="uint" value="0"/>
    </property>
    <property name="plugin-9" type="string" value="clock">
      <property name="mode" type="uint" value="2"/>
      <property name="digital-layout" type="uint" value="1"/>
      <property name="digital-time-format" type="string" value="%R"/>
      <property name="digital-date-format" type="string" value="%a %b %-d"/>
    </property>
    <property name="plugin-10" type="string" value="actions">
      <property name="appearance" type="uint" value="1"/>
      <property name="button-title" type="uint" value="3"/>
      <property name="custom-title" type="string" value="sam"/>
    </property>
    <property name="plugin-11" type="string" value="separator">
      <property name="style" type="uint" value="0"/>
    </property>"""

# the status tray's icons, as an app would put them there
TRAY_ICONS = ("audio-volume-high-symbolic", "network-wireless-signal-excellent-symbolic")
TRAY = """
import sys
import gi
gi.require_version("Gtk", "3.0")
from gi.repository import Gtk
icons = [Gtk.StatusIcon.new_from_icon_name(name) for name in sys.argv[1:]]
Gtk.main()
"""

WM_BASE_THEME = "Material-Black-Pistachio"  # square corners
WM_THEME = "TukzDemo"
DECO_BG = "#44506b"
DECO_BG_OFF = "#2e3547"
DECO_GLYPH = "#dde5f2"
DECO_TEXT = "#e9eefa"

# Sister tooling has :93 and :95 up, so the search starts lower and steps down
# past a taken one.
DEFAULT_DISPLAY = ":89"
DISPLAY_TRIES = 6

# where the panel sees the throwaway home and the plugin, through bwrap
DEMO_HOME = "/home/sam"
DEMO_PREFIX = "/opt/tukzmon"

# every process the run starts has this in its environment, so cleanup finds
# the settings window and anything else the panel spawned
MARK = "TUKZMON_DEMO_RUN"
DESK_MARK = "TUKZMON_DEMO_DESK"


class Take(enum.Enum):
    VIDEO = "video"
    GIF = "gif"


@dataclass(frozen=True)
class Profile:
    take: Take
    scale: int
    cap_fps: int
    out_fps: int
    banner_fs: int
    banner_min: float
    tail_s: float  # padded on after the scenes

    @property
    def size(self) -> tuple[int, int]:
        return VIEW_W * self.scale, VIEW_H * self.scale

    @property
    def band(self) -> int:
        return BAND * self.scale


PROFILES = {
    Take.VIDEO: Profile(Take.VIDEO, 2, 60, 60, 34, 3.0, TAIL_HOLD_S + VIDEO_TAIL_BLACK_S),
    Take.GIF: Profile(Take.GIF, 1, 50, 50, 20, 2.5, TAIL_HOLD_S + GIF_TAIL_BLACK_S),
}


@dataclass(frozen=True)
class Options:
    display: str
    takes: list[Take]
    scenes: list[str]
    lib_dir: Path
    history_bin: Path
    keep_work: bool
    no_rotate: bool
    no_asset: bool
    out_dir: Path
    shot: Path | None


def log(msg: str) -> None:
    print(f"[demo] {msg}", flush=True)


def run(cmd: list[str], **kw: object) -> None:
    subprocess.run(cmd, check=True, **kw)  # type: ignore[call-overload]


def out_of(cmd: list[str], **kw: object) -> str:
    done = subprocess.run(cmd, check=True, capture_output=True, text=True, **kw)  # type: ignore[call-overload]
    return str(done.stdout)


def display_taken(num: int | str) -> bool:
    lock = Path(f"/tmp/.X{num}-lock")
    if not lock.exists():
        return False
    try:
        pid = int(lock.read_text(encoding="utf-8").strip())
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except (ValueError, OSError):
        return True
    return True


def pick_display(wanted: str, explicit: bool) -> str:
    if explicit or not display_taken(wanted.lstrip(":")):
        return wanted
    base = int(wanted.lstrip(":"))
    for num in range(base - 1, base - DISPLAY_TRIES, -1):
        if not display_taken(num):
            log(f"display {wanted} is taken; using :{num}")
            return f":{num}"
    raise RuntimeError(f"no free display from {wanted} down {DISPLAY_TRIES}")


def project_temp() -> Path:
    host = Path(os.environ.get("HOST_TEMP") or os.environ.get("TMPDIR") or "/tmp").resolve()
    return host / "tukzmon"


def temp_root() -> tuple[Path, bool]:
    """The run folder: cicd's when it set TMPDIR to one, else a new one."""
    proj = project_temp()
    tmp = Path(os.environ["TMPDIR"]).resolve() if os.environ.get("TMPDIR") else None
    if tmp and tmp.is_relative_to(proj) and tmp != proj:
        base = tmp / "demo"
        base.mkdir(parents=True, exist_ok=True)
        return base, False
    stamp = time.strftime("%Y%m%d-%H%M%S")
    for n in range(1, 1000):
        base = proj / f"{stamp}_{n:03d}"
        try:
            base.mkdir(parents=True)
            return base, True
        except FileExistsError:
            continue
    raise RuntimeError(f"no free run folder under {proj}")


def safe_rmtree(path: Path) -> None:
    # Only a folder this run made: real, under <temp>/tukzmon/, below the
    # project folder itself, and holding the run's marker file.
    proj = project_temp()
    if path.is_symlink() or not path.is_dir():
        log(f"WARNING: not removing {path}: not a plain folder")
        return
    real = path.resolve()
    if not real.is_relative_to(proj) or real == proj or "tmp" not in real.parts:
        log(f"WARNING: not removing {path}: outside {proj}")
        return
    if not (real / ".demo-run").is_file():
        log(f"WARNING: not removing {path}: no run marker")
        return
    shutil.rmtree(real)


##•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
##	Processes

# A pidfd never points at a reused pid, and polling one waits with no scan.


@dataclass(frozen=True)
class Marked:
    pid: int
    args: str


def marked(mark: str, work: Path) -> list[Marked]:
    """Every live process with this mark for this run, from one pass over /proc."""
    want = f"{mark}={work}".encode()
    found = []
    for d in Path("/proc").iterdir():
        if not d.name.isdigit() or int(d.name) == os.getpid():
            continue
        try:
            if want in (d / "environ").read_bytes().split(b"\0"):
                args = (d / "cmdline").read_bytes().replace(b"\0", b" ").decode(errors="replace")
                found.append(Marked(int(d.name), args))
        except OSError:
            continue  # gone, or not ours to read
    return found


class Pidfds(contextlib.AbstractContextManager["Pidfds"]):
    def __init__(self, pids: Iterable[int]) -> None:
        self.fds: dict[int, int] = {}
        for pid in pids:
            with contextlib.suppress(ProcessLookupError):
                self.fds[pid] = os.pidfd_open(pid)

    def __exit__(self, kind: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None) -> None:
        for fd in self.fds.values():
            os.close(fd)
        self.fds.clear()

    def wait(self, timeout: float, pids: Iterable[int] | None = None) -> list[int]:
        """Waits for them to exit, and gives the pids still running at the timeout."""
        left = {self.fds[pid]: pid for pid in (self.fds if pids is None else pids) if pid in self.fds}
        poller = select.poll()
        for fd in left:
            poller.register(fd, select.POLLIN)
        deadline = time.monotonic() + timeout
        while left:
            ms = (deadline - time.monotonic()) * 1000
            if ms <= 0:
                break
            for fd, _ in poller.poll(ms):
                poller.unregister(fd)
                del left[fd]
        return list(left.values())

    def send(self, sig: signal.Signals, pids: Iterable[int] | None = None) -> None:
        for pid in self.fds if pids is None else pids:
            with contextlib.suppress(ProcessLookupError):
                signal.pidfd_send_signal(self.fds[pid], sig)

    def stop(self, grace: float = 3.0) -> None:
        self.send(signal.SIGTERM)
        self.send(signal.SIGKILL, self.wait(grace))


def kill_group(proc: subprocess.Popen[bytes]) -> None:
    for sig in (signal.SIGTERM, signal.SIGKILL):
        try:
            os.killpg(proc.pid, sig)
        except ProcessLookupError:
            break
        try:
            proc.wait(timeout=5)
            break
        except subprocess.TimeoutExpired:
            continue
    # dbus-run-session's bus and helpers can outlive their leader
    with contextlib.suppress(ProcessLookupError):
        os.killpg(proc.pid, signal.SIGTERM)


##•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
##	The take's folder, display and window manager


class Work:
    def __init__(self, path: Path) -> None:
        self.path = path
        path.mkdir(parents=True, exist_ok=True)
        (path / ".demo-run").touch()
        self.logs: list[IO[str]] = []

    def log_to(self, name: str, mode: str = "w") -> IO[str]:
        # Popen keeps writing after this returns, so close() closes it
        f = open(self.path / name, mode, encoding="utf-8")  # noqa: SIM115
        self.logs.append(f)
        return f

    def close(self) -> None:
        for f in self.logs:
            f.close()
        self.logs.clear()


class Screen:
    def __init__(self, display: str, profile: Profile, work: Work) -> None:
        self.display = display
        self.num = display.lstrip(":")
        self.scale = profile.scale
        self.size = profile.size
        self.band = profile.band
        self.work = work
        self.xvfb: subprocess.Popen[bytes] | None = None
        self.wm: subprocess.Popen[bytes] | None = None
        self.x: xdisplay.Display | None = None

    def env(self) -> dict[str, str]:
        e = dict(os.environ)
        e.update(DISPLAY=self.display)
        e[MARK] = str(self.work.path)
        # with these set a GTK or Qt app would open on the real desktop
        for k in ("WAYLAND_DISPLAY", "XDG_SESSION_TYPE", "DBUS_SESSION_BUS_ADDRESS", "XAUTHORITY"):
            e.pop(k, None)
        return e

    def px(self, lx: float, ly: float) -> tuple[int, int]:
        return int(lx * self.scale), int(ly * self.scale)

    def xdo(self, *a: str) -> None:
        run(["xdotool", *a], env=self.env(), stdout=subprocess.DEVNULL)

    def pointer(self, x: int, y: int) -> None:
        # a motion event, as xdotool sends, without a process for each step
        if self.x is None:
            raise RuntimeError("the display isn't open")
        self.x.xtest_fake_input(X.MotionNotify, x=x, y=y)
        self.x.sync()

    def park_pointer(self) -> None:
        # the arrow's tip is its hotspot, so in the corner none of it shows
        self.pointer(self.size[0] - 1, self.size[1] - 1)

    def set_root(self, color: str) -> None:
        run(["xsetroot", "-solid", color], env=self.env())

    def start(self) -> None:
        if display_taken(self.num):
            raise RuntimeError(f"display {self.display} is taken")
        self.xvfb = subprocess.Popen(
            [
                "Xvfb",
                self.display,
                "-screen",
                "0",
                f"{self.size[0]}x{self.size[1]}x24",
                "-dpi",
                "96",
                "-nolisten",
                "tcp",
            ],
            stdout=subprocess.DEVNULL,
            stderr=self.work.log_to("xvfb.log"),
            start_new_session=True,
        )
        lock = Path(f"/tmp/.X{self.num}-lock")
        deadline = time.time() + 10
        while time.time() < deadline:
            if lock.exists() and lock.read_text(encoding="utf-8").strip() == str(self.xvfb.pid):
                break
            time.sleep(0.1)
        else:
            raise RuntimeError(f"Xvfb on {self.display} never came up as ours (see xvfb.log)")
        time.sleep(0.5)
        self.x = xdisplay.Display(self.display)
        if not self.x.has_extension("XTEST"):
            raise RuntimeError(f"Xvfb on {self.display} has no XTEST")
        self.start_wm()
        time.sleep(1.5)
        self.reserve_band()
        # after the WM, which paints the root black as it starts
        self.set_root(DESK)

    def make_theme(self, wmhome: Path) -> str:
        base = Path("/usr/share/themes") / WM_BASE_THEME / "xfwm4"
        if not base.is_dir():
            log(f"WARNING: theme {WM_BASE_THEME} not installed, so the WM default")
            return "Default"
        dst = wmhome / ".themes" / WM_THEME / "xfwm4"
        dst.parent.mkdir(parents=True, exist_ok=True)
        shutil.copytree(base, dst, dirs_exist_ok=True)
        for svg in dst.rglob("*.svg"):
            text = svg.read_text(encoding="utf-8")
            svg.write_text(
                text.replace("#09090a", DECO_BG).replace("#1a1c1e", DECO_BG_OFF).replace("#a3a3a3", DECO_GLYPH),
                encoding="utf-8",
            )
        rc = dst / "themerc"
        rc.write_text(
            re.sub(r"(?m)^(active_text(_shadow)?_color)=.*", rf"\1={DECO_TEXT}", rc.read_text(encoding="utf-8")),
            encoding="utf-8",
        )
        return WM_THEME

    def start_wm(self) -> None:
        wmhome = self.work.path / "wmhome"
        e = self.env()
        e["HOME"] = str(wmhome)
        for var, sub in (
            ("XDG_CONFIG_HOME", ".config"),
            ("XDG_DATA_HOME", ".local/share"),
            ("XDG_CACHE_HOME", ".cache"),
            ("XDG_RUNTIME_DIR", "run"),
        ):
            d = wmhome / sub
            d.mkdir(parents=True, exist_ok=True, mode=0o700)
            e[var] = str(d)
        theme = self.make_theme(wmhome)
        title_pt = 9 * self.scale
        self.wm = subprocess.Popen(
            [
                "dbus-run-session",
                "--",
                "sh",
                "-c",
                f'xfconf-query -c xfwm4 -p /general/theme --create -t string -s "{theme}"; '
                f'xfconf-query -c xfwm4 -p /general/title_font --create -t string -s "Lato Bold {title_pt}"; '
                'xfconf-query -c xfwm4 -p /general/button_layout --create -t string -s "O|HMC"; '
                "xfconf-query -c xfwm4 -p /general/placement_ratio --create -t int -s 100; "
                "xfconf-query -c xfwm4 -p /general/placement_mode --create -t string -s center; "
                "exec xfwm4 --compositor=off --vblank=off",
            ],
            env=e,
            stdout=self.work.log_to("wm.log"),
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )

    def reserve_band(self) -> None:
        # a dock with a strut, so the WM keeps windows out of the caption band.
        # It lasts as long as this connection.
        if self.x is None:
            raise RuntimeError("the display isn't open")
        width, height = self.size
        scr = self.x.screen()
        win = scr.root.create_window(
            0,
            height - 1,
            1,
            1,
            0,
            scr.root_depth,
            X.InputOutput,
            X.CopyFromParent,
            background_pixel=scr.black_pixel,
        )
        atom = self.x.intern_atom
        win.change_property(atom("_NET_WM_WINDOW_TYPE"), atom("ATOM"), 32, [atom("_NET_WM_WINDOW_TYPE_DOCK")])
        win.change_property(atom("_NET_WM_STRUT"), atom("CARDINAL"), 32, [0, 0, 0, self.band])
        win.change_property(
            atom("_NET_WM_STRUT_PARTIAL"), atom("CARDINAL"), 32, [0, 0, 0, self.band, 0, 0, 0, 0, 0, 0, 0, width - 1]
        )
        win.map()
        self.x.sync()
        time.sleep(0.6)

    def win_ids(self, *search: str) -> list[str]:
        # xdotool search exits 1 when nothing matches yet
        done = subprocess.run(["xdotool", "search", *search], env=self.env(), capture_output=True, text=True)
        return done.stdout.split()

    def client_geom(self, win: str) -> tuple[int, int, int, int]:
        info = out_of(["xwininfo", "-id", win], env=self.env())

        def num(key: str) -> int:
            mo = re.search(rf"{key}:\s*(-?\d+)", info)
            if not mo:
                raise RuntimeError(f"xwininfo gave no {key} for {win}")
            return int(mo.group(1))

        return num("Absolute upper-left X"), num("Absolute upper-left Y"), num("Width"), num("Height")

    def frame_extents(self, win: str) -> tuple[int, int, int, int]:
        text = out_of(["xprop", "-id", win, "_NET_FRAME_EXTENTS"], env=self.env())
        mo = re.search(r"=\s*(\d+),\s*(\d+),\s*(\d+),\s*(\d+)", text)
        if not mo:
            # not set yet; place_settings measures and corrects anyway
            return 2, 2, 32, 2
        left, right, top, bottom = (int(g) for g in mo.groups())
        return left, right, top, bottom

    def shot(self, path: Path) -> None:
        run(["import", "-window", "root", str(path)], env=self.env())
        log(f"screenshot: {path}")

    def close(self) -> None:
        if self.x is not None:
            self.x.close()
            self.x = None
        for proc in (self.wm, self.xvfb):
            if proc:
                kill_group(proc)
        self.wm = self.xvfb = None


##•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
##	The panel and the settings window


class Panel:
    def __init__(self, screen: Screen, lib_dir: Path, history_bin: Path) -> None:
        self.screen = screen
        self.work = screen.work.path
        self.lib_dir = lib_dir
        self.history_bin = history_bin
        self.home = self.work / "home"
        self.prefix = self.work / "prefix"
        self.days = self.work / "days.hist"
        self.proc: subprocess.Popen[bytes] | None = None
        self.item_x = 0  # tukzmon's left edge in the panel, gif px

    @property
    def hist(self) -> Path:
        return self.home / ".local/share/tukzmon/plugin-1.hist"

    def desk_env(self) -> dict[str, str]:
        # the panel, the plugin, and the settings window the plugin starts
        e = self.screen.env()
        e[DESK_MARK] = str(self.work)
        h = Path(DEMO_HOME)
        scale = str(self.screen.scale)
        e.update(
            HOME=str(h),
            XDG_CONFIG_HOME=str(h / ".config"),
            XDG_DATA_HOME=str(h / ".local/share"),
            XDG_CACHE_HOME=str(h / ".cache"),
            XDG_STATE_HOME=str(h / ".local/state"),
            XDG_RUNTIME_DIR=str(h / "run"),
            XDG_DATA_DIRS=f"{DEMO_PREFIX}/share:/usr/share",
            GDK_SCALE=scale,
            GDK_BACKEND="x11",
            GTK_THEME="Adwaita:dark",
            QT_QPA_PLATFORM="xcb",
            QT_QPA_PLATFORMTHEME="gtk3",
            QT_SCALE_FACTOR=scale,
            QT_ENABLE_HIGHDPI_SCALING="0",
            NO_AT_BRIDGE="1",
        )
        for k in ("GDK_DPI_SCALE", "QT_AUTO_SCREEN_SCALE_FACTOR", "QT_SCREEN_SCALE_FACTORS", "QT_FONT_DPI"):
            e.pop(k, None)
        return e

    def prep_home(self) -> None:
        h = self.home
        for d in (".config/xfce4/xfconf/xfce-perchannel-xml", ".config/gtk-3.0", ".local/share", ".cache", "run"):
            (h / d).mkdir(parents=True, exist_ok=True)
        (h / "run").chmod(0o700)
        share = self.prefix / "share/xfce4/panel/plugins"
        share.mkdir(parents=True, exist_ok=True)
        shutil.copy2(REPO / "xfce/tukzmon.desktop", share)
        # 4.20 pairs <prefix>/share with <prefix>/lib/xfce4/panel/plugins, and
        # this box's build with the multiarch folder too
        for sub in ("lib/xfce4/panel/plugins", "lib/x86_64-linux-gnu/xfce4/panel/plugins"):
            plugins = self.prefix / sub
            plugins.mkdir(parents=True, exist_ok=True)
            for f in ("libtukzmon.so", "tukzmon-settings"):
                shutil.copy2(self.lib_dir / f, plugins)
        (h / ".config/gtk-3.0/settings.ini").write_text(
            "[Settings]\ngtk-theme-name=Adwaita\ngtk-application-prefer-dark-theme=1\n"
            "gtk-font-name=Lato 10\ngtk-enable-animations=0\n",
            encoding="utf-8",
        )
        ids = "\n".join(f'        <value type="int" value="{n}"/>' for n in PANEL_ORDER)
        panel = f"""<?xml version="1.0" encoding="UTF-8"?>
<channel name="xfce4-panel" version="1.0">
  <property name="configver" type="int" value="2"/>
  <property name="panels" type="array">
    <value type="int" value="1"/>
    <property name="panel-1" type="empty">
      <property name="position" type="string" value="p=6;x=0;y=0"/>
      <property name="length" type="double" value="100"/>
      <property name="position-locked" type="bool" value="true"/>
      <property name="size" type="uint" value="{PANEL}"/>
      <property name="icon-size" type="uint" value="24"/>
      <property name="plugin-ids" type="array">
{ids}
      </property>
    </property>
  </property>
  <property name="plugins" type="empty">
{PANEL_PLUGINS}
  </property>
</channel>
"""
        (h / ".config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml").write_text(panel, encoding="utf-8")

    def sandbox(self) -> list[str]:
        # the throwaway home and plugin prefix, mounted where their paths look
        # ordinary, since the settings window's title shows its file's path
        return [
            "bwrap",
            "--dev-bind",
            "/",
            "/",
            "--tmpfs",
            "/home",
            "--tmpfs",
            "/opt",
            "--bind",
            str(self.home),
            DEMO_HOME,
            "--ro-bind",
            str(self.prefix),
            DEMO_PREFIX,
            "--chdir",
            DEMO_HOME,
            "--die-with-parent",
            "--",
        ]

    def start(self) -> None:
        logs = self.screen.work
        self.proc = subprocess.Popen(
            [*self.sandbox(), "dbus-run-session", "--", "xfce4-panel"],
            env=self.desk_env(),
            stdout=logs.log_to("panel.log", "a"),
            stderr=subprocess.STDOUT,
            stdin=subprocess.DEVNULL,
            start_new_session=True,
        )
        # has the panel side's mark, so stop() takes it down too
        subprocess.Popen(
            [*self.sandbox(), sys.executable, "-c", TRAY, *TRAY_ICONS],
            env=self.desk_env(),
            stdout=logs.log_to("tray.log", "a"),
            stderr=subprocess.STDOUT,
            stdin=subprocess.DEVNULL,
            start_new_session=True,
        )

    def plugin_pids(self) -> list[int]:
        return [p.pid for p in marked(MARK, self.work) if "wrapper-2.0" in p.args and "libtukzmon" in p.args]

    def wait(self, timeout: float = 20) -> None:
        deadline = time.time() + timeout
        while time.time() < deadline:
            if self.screen.win_ids("--onlyvisible", "--class", "xfce4-panel"):
                pids = self.plugin_pids()
                if pids:
                    self.item_x = self.find_item(pids)
                    return
            time.sleep(0.2)
        raise RuntimeError("the panel never showed (see panel.log)")

    def find_item(self, pids: list[int], timeout: float = 10) -> int:
        # the items beside it set where it lands, so it's measured rather than fixed
        scale = self.screen.scale
        deadline = time.time() + timeout
        while time.time() < deadline:
            for pid in pids:
                for win in self.screen.win_ids("--all", "--onlyvisible", "--pid", str(pid)):
                    x, y, w, _ = self.screen.client_geom(win)
                    if w > 50 and y < PANEL * scale:
                        return x // scale
            time.sleep(0.2)
        raise RuntimeError("couldn't find tukzmon in the panel")

    def stop(self) -> None:
        # One scan, before the kill, since the bus's helpers such as xfconfd
        # outlive the group. The plugin saves its history on the way out, so
        # it's waited for before anything writes that file.
        procs = marked(DESK_MARK, self.work)
        with Pidfds(p.pid for p in procs) as fds:
            if self.proc:
                kill_group(self.proc)
                self.proc = None
            fds.wait(5.0, [p.pid for p in procs if "wrapper-2.0" in p.args])
            fds.stop()

    def make_history(self, days: float = 4.0) -> None:
        # written while the panel still runs, so it's down only for the copy
        run([str(self.history_bin), str(self.days), str(days)], env=self.screen.env())

    def put_history(self) -> None:
        self.hist.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(self.days, self.hist)


class SettingsWin:
    """The settings window, with points in it in gif px from its client corner."""

    def __init__(self, screen: Screen, timeout: float = 20) -> None:
        self.screen = screen
        self.win = self.find(timeout)
        self.origin = (0, 0)
        _, _, w, h = screen.client_geom(self.win)
        self.size = (w, h)

    def find(self, timeout: float) -> str:
        deadline = time.time() + timeout
        while time.time() < deadline:
            for win in self.screen.win_ids("--onlyvisible", "--class", "tukzmon-settings"):
                if self.screen.client_geom(win)[2] > 200:
                    return win
            time.sleep(0.2)
        raise RuntimeError("the settings window never showed")

    def place(self) -> None:
        # centered in the room between the panel and the band
        screen = self.screen
        left, right, top, bottom = screen.frame_extents(self.win)
        w, h = self.size
        room_t = PANEL * screen.scale
        room_h = screen.size[1] - room_t - screen.band
        want_x = max(0, (screen.size[0] - (w + left + right)) // 2) + left
        want_y = room_t + max(0, (room_h - (h + top + bottom)) // 2) + top
        target = [want_x, want_y]
        for _ in range(4):
            screen.xdo("windowmove", self.win, str(target[0] - left), str(target[1] - top))
            time.sleep(0.25)
            cx, cy, _, _ = screen.client_geom(self.win)
            dx, dy = want_x - cx, want_y - cy
            if abs(dx) <= 1 and abs(dy) <= 1:
                break
            target[0] += dx
            target[1] += dy
        cx, cy, _, _ = screen.client_geom(self.win)
        self.origin = (cx, cy)

    def sp(self, lx: float, ly: float) -> tuple[int, int]:
        x, y = self.screen.px(lx, ly)
        return self.origin[0] + x, self.origin[1] + y

    def corner(self, x: float, y: float) -> tuple[float, float]:
        """A point from the bottom right, in gif px from the client corner."""
        return self.size[0] / self.screen.scale + x, self.size[1] / self.screen.scale + y


##•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
##	The timeline and the capture


class Sound(enum.Enum):
    CLICK = "click"
    CLICK_QUIET = "click_quiet"


@dataclass(frozen=True)
class Event:
    at: float  # epoch
    sound: Sound


@dataclass
class Caption:
    start: float
    end: float
    text: str


@dataclass
class Timeline:
    events: list[Event] = field(default_factory=list)
    captions: list[Caption] = field(default_factory=list)
    t0_e: float = 0.0
    flash_e: float = 0.0
    flash2_e: float = 0.0
    flash_vt: float = 0.0
    flash2_vt: float = 0.0

    def to_vt(self, epoch: float) -> float:
        """Wall clock to the capture's time, between the 2 flashes, since a
        busy box can run the capture a percent or so slow."""
        rate = (self.flash2_vt - self.flash_vt) / (self.flash2_e - self.flash_e)
        return self.flash_vt + (epoch - self.flash_e) * rate


class Capture:
    def __init__(self, screen: Screen, timeline: Timeline, fps: int) -> None:
        self.screen = screen
        self.timeline = timeline
        self.fps = fps
        self.raw = screen.work.path / "raw.mkv"
        self.ff: subprocess.Popen[bytes] | None = None

    def start(self) -> None:
        screen = self.screen
        prog = screen.work.path / "ffprogress.txt"
        self.ff = subprocess.Popen(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-progress",
                str(prog),
                "-f",
                "x11grab",
                "-draw_mouse",
                "1",
                "-framerate",
                str(self.fps),
                "-video_size",
                f"{screen.size[0]}x{screen.size[1]}",
                "-i",
                screen.display,
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-qp",
                "0",
                "-pix_fmt",
                "yuv444p",
                str(self.raw),
            ],
            env=screen.env(),
            stdin=subprocess.DEVNULL,
            stderr=screen.work.log_to("ffmpeg.log"),
        )
        deadline = time.time() + 30
        while time.time() < deadline:
            if prog.exists() and re.search(r"(?m)^frame=([1-9]\d*)", prog.read_text(encoding="utf-8")):
                break
            time.sleep(0.3)
        else:
            raise RuntimeError("x11grab gave no frames (see ffmpeg.log)")
        time.sleep(0.8)
        self.timeline.flash_e = self.flash()
        screen.park_pointer()
        time.sleep(0.4)

    def flash(self) -> float:
        self.screen.set_root("white")
        at = time.time()
        time.sleep(0.25)
        self.screen.set_root(DESK)
        return at

    def stop(self) -> None:
        if self.ff:
            self.ff.send_signal(signal.SIGINT)
            try:
                self.ff.wait(timeout=30)
            except subprocess.TimeoutExpired:
                self.ff.kill()
            self.ff = None


##•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
##	A take, the mouse and captions


class Rec:
    def __init__(self, opts: Options, profile: Profile, base: Path) -> None:
        self.profile = profile
        self.scale = profile.scale
        self.work = Work(base / f"take-{self.scale}x")
        self.screen = Screen(opts.display, profile, self.work)
        self.panel = Panel(self.screen, opts.lib_dir.resolve(), opts.history_bin.resolve())
        self.timeline = Timeline()
        self.capture = Capture(self.screen, self.timeline, profile.cap_fps)
        self.settings: SettingsWin | None = None
        self.keep = opts.keep_work
        # --shot also saves a picture at each peek, beside the shot
        self.peeks = opts.shot.with_suffix("") if opts.shot else None

    def start(self) -> None:
        self.panel.prep_home()
        self.screen.start()

    def peek(self, name: str) -> None:
        if self.peeks:
            self.screen.shot(Path(f"{self.peeks}-{name}.png"))

    def cleanup(self) -> None:
        self.capture.stop()
        self.panel.stop()
        with Pidfds(p.pid for p in marked(MARK, self.work.path)) as fds:
            fds.stop()
        self.screen.close()
        self.work.close()
        if not self.keep and self.work.path.exists():
            safe_rmtree(self.work.path)


class Mouse:
    def __init__(self, rec: Rec) -> None:
        self.rec = rec
        self.pos = (rec.screen.size[0] - 1, rec.screen.size[1] - 1)

    def move(self, lx: float, ly: float, dur: float = 0.45) -> None:
        self.move_px(*self.rec.screen.px(lx, ly), dur)

    def move_px(self, x: int, y: int, dur: float = 0.45) -> None:
        x0, y0 = self.pos
        steps = max(6, int(dur * 50))
        began = time.time()
        for i in range(1, steps + 1):
            t = i / steps
            t = t * t * (3 - 2 * t)
            self.rec.screen.pointer(int(x0 + (x - x0) * t), int(y0 + (y - y0) * t))
            time.sleep(max(0.0, began + dur * i / steps - time.time()))
        self.pos = (x, y)

    def click(self, button: str = "1", quiet: bool = False) -> None:
        # logged first, since the press goes out at the start of the call
        self.rec.timeline.events.append(Event(time.time(), Sound.CLICK_QUIET if quiet else Sound.CLICK))
        self.rec.screen.xdo("click", button)

    def at(self, lx: float, ly: float, dur: float = 0.45, settle: float = 0.45, button: str = "1") -> None:
        self.move(lx, ly, dur)
        time.sleep(0.12)
        self.click(button)
        time.sleep(settle)

    def at_win(self, win: SettingsWin, at: tuple[float, float], dur: float = 0.45, settle: float = 0.45) -> None:
        self.move_px(*win.sp(*at), dur)
        time.sleep(0.12)
        self.click()
        time.sleep(settle)


class Banner(contextlib.AbstractContextManager["Banner"]):
    def __init__(self, rec: Rec, text: str) -> None:
        self.rec, self.text = rec, text
        self.start = 0.0

    def __enter__(self) -> "Banner":
        self.start = time.time()
        return self

    def __exit__(self, kind: type[BaseException] | None, exc: BaseException | None, tb: TracebackType | None) -> None:
        self.rec.timeline.captions.append(Caption(self.start, time.time(), self.text))


##•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
##	Scenes, in gif px. script.txt says what each one shows.

# panel points are from tukzmon's left edge, at the default graphs and a 56 px panel
CPU_X = (2, 102)
GRAPH_Y = 28
TIP_STOPS = (96, 40, 3)  # across the CPU graph, newest to oldest

MENU_AT = 60
PROPS_ITEM = (118, 100)  # the item menu's Properties, with the menu opened at (MENU_AT, GRAPH_Y)
GRAPHS_TAB = (35, 60)  # in the settings window
NET_SHOW = (121, 209)  # the network row's show box
THEMES_TAB = (37, 92)
THEME_PICK = (205, 37)  # the theme name dropdown
NEON = (189, 110)  # in its open list
APPLY_BUTTON = (-140, -28)  # from the window's bottom right
OK_BUTTON = (-53, -28)  # from the window's bottom right


def scene_fresh(r: Rec, m: Mouse) -> None:
    r.panel.start()
    r.panel.wait()
    with Banner(r, "tukzmon: system graphs for the XFCE panel"):
        time.sleep(2.6)
    with Banner(r, "From a fresh start, the graphs fill in and settle"):
        time.sleep(4.0)


def scene_days(r: Rec, m: Mouse) -> None:
    panel = r.panel
    with Banner(r, "A few days later, the log time axis still fits them all"):
        panel.make_history()
        panel.stop()
        panel.put_history()
        panel.start()
        panel.wait()
        time.sleep(2.2)
    with Banner(r, "Hover for the value, and how long ago"):
        m.move(panel.item_x + TIP_STOPS[0], GRAPH_Y, 0.6)
        time.sleep(1.5)
        r.peek(f"tip{TIP_STOPS[0]}")
        for x in TIP_STOPS[1:]:
            m.move(panel.item_x + x, GRAPH_Y, 0.5)
            time.sleep(1.4)
            r.peek(f"tip{x}")


def scene_settings(r: Rec, m: Mouse) -> None:
    item_x = r.panel.item_x
    with Banner(r, "Right-click, Properties"):
        m.at(item_x + MENU_AT, GRAPH_Y, button="3", settle=0.8)
        r.peek("menu")
        m.at(item_x + PROPS_ITEM[0], PROPS_ITEM[1], settle=0.3)
        win = r.settings = SettingsWin(r.screen)
        win.place()
        time.sleep(0.8)
        r.peek("open")
    with Banner(r, "The Graphs tab: which graphs, in what order, how wide"):
        m.at_win(win, GRAPHS_TAB, settle=1.6)
        r.peek("graphs")
        m.at_win(win, NET_SHOW, settle=0.5)
        m.at_win(win, win.corner(*APPLY_BUTTON), settle=1.6)
        r.peek("net")
    with Banner(r, "One theme colors every graph"):
        m.at_win(win, THEMES_TAB, settle=0.7)
        m.at_win(win, THEME_PICK, settle=0.6)
        r.peek("list")
        m.at_win(win, NEON, settle=0.4)
        m.at_win(win, win.corner(*APPLY_BUTTON), settle=1.8)
        r.peek("neon")
        m.at_win(win, win.corner(*OK_BUTTON), settle=1.0)


def scene_outro(r: Rec, m: Mouse) -> None:
    with Banner(r, "github.com/tukzedo/tukzmon"):
        m.move(VIEW_W - 120, 300, 0.5)
        time.sleep(1.2)


Scene = Callable[[Rec, Mouse], None]
SCENES: dict[str, Scene] = {
    "fresh": scene_fresh,
    "days": scene_days,
    "settings": scene_settings,
    "outro": scene_outro,
}


##•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
##	Audio: the clicks, mixed from the event log

SOUND_FILES = {
    Sound.CLICK: SOUNDS / "mouse/click.wav",
    Sound.CLICK_QUIET: SOUNDS / "mouse/click_quiet.wav",
}
GAIN = {Sound.CLICK: 0.55, Sound.CLICK_QUIET: 0.38}


def load_samples(work: Path) -> dict[Sound, np.ndarray]:
    cache = {}
    for sound, path in SOUND_FILES.items():
        wav = work / f"{sound.value}.wav"
        run(["ffmpeg", "-v", "error", "-y", "-i", str(path), "-ar", str(SR), "-ac", "2", "-f", "wav", str(wav)])
        with wave.open(str(wav), "rb") as w:
            data = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16)
        pcm = data.astype(np.float32).reshape(-1, 2) / 32768.0
        # the files have about 0.16 s of quiet before the click itself
        loud = np.abs(pcm).max(axis=1)
        onset = int(np.argmax(loud > 0.2 * loud.max()))
        cache[sound] = pcm[max(0, onset - SR // 500) :]
    return cache


def build_audio(rec: Rec, trim: float, duration: float, rng: random.Random) -> Path:
    work = rec.work.path
    cache = load_samples(work)
    mix = np.zeros((int(duration * SR) + SR, 2), dtype=np.float32)
    for event in rec.timeline.events:
        t_rel = rec.timeline.to_vt(event.at) - trim + FOLEY_LAG
        if t_rel < -0.5 or t_rel > duration:
            continue
        clip = cache[event.sound]
        gain = GAIN[event.sound] * rng.uniform(0.85, 1.05)
        at = int(max(0.0, t_rel) * SR)
        end = min(at + len(clip), len(mix))
        mix[at:end] += clip[: end - at] * gain
    peak = np.abs(mix).max()
    if peak > 0:
        mix *= min(0.40 / peak, 4.0)
    out = work / "audio.wav"
    with wave.open(str(out), "wb") as w:
        w.setnchannels(2)
        w.setsampwidth(2)
        w.setframerate(SR)
        w.writeframes((mix * 32767.0).astype(np.int16).tobytes())
    return out


##•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
##	Post: find the flash, captions, encode


def find_flash(raw: Path, work: Path, start: float = 0.0) -> float:
    """When the white flash starts, in the 8 s from start."""
    stats = work / "stats.txt"
    run(
        [
            "ffmpeg",
            "-v",
            "error",
            "-ss",
            f"{start:.3f}",
            "-t",
            "8",
            "-i",
            str(raw),
            "-copyts",
            "-vf",
            f"signalstats,metadata=print:key=lavfi.signalstats.YAVG:file={stats}",
            "-f",
            "null",
            "-",
        ]
    )
    seen, pts = [], 0.0
    for line in stats.read_text(encoding="utf-8").splitlines():
        mo = re.search(r"pts_time:([0-9.]+)", line)
        if mo:
            pts = float(mo.group(1))
        mo = re.search(r"YAVG=([0-9.]+)", line)
        if mo:
            seen.append((pts, float(mo.group(1))))
    top = max((y for _, y in seen), default=0.0)
    if top < 180:
        raise RuntimeError(f"sync flash not found (max YAVG {top})")
    # its first frame, since the flash lasts a quarter second
    low = min(y for _, y in seen)
    return next(t for t, y in seen if y >= (low + top) / 2)


def vf_chain(rec: Rec, trim: float, dur: float) -> str:
    p = rec.profile
    to_vt = rec.timeline.to_vt
    # the capture runs on past the scenes to the end flash, and the tail is padded on after this
    filters = [f"trim=end={dur:.3f}", f"fps={p.out_fps}"]
    spans = []
    for cap in rec.timeline.captions:
        s = max(0.0, to_vt(cap.start) - trim)
        spans.append(Caption(s, max(s + p.banner_min, to_vt(cap.end) - trim), cap.text))
    spans.sort(key=lambda span: span.start)
    # one caption at a time
    for span, after in itertools.pairwise(spans):
        span.end = min(span.end, after.start - 0.3)
    y = f"h-{p.band}+({p.band}-text_h)/2"
    for i, span in enumerate(spans):
        s, e = span.start, span.end
        if e <= s:
            continue
        tf = rec.work.path / f"banner{i}.txt"
        tf.write_text(span.text, encoding="utf-8")
        fade = f"clip((t-{s:.3f})/0.15,0,1)*clip(({e:.3f}-t)/0.15,0,1)"
        filters.append(
            f"drawtext=fontfile={BANNER_TTF}:textfile={tf}:fontsize={p.banner_fs}:"
            f"fontcolor={BANNER_FG}:x=(w-text_w)/2:y={y}:alpha='{fade}':enable='between(t,{s:.3f},{e:.3f})'"
        )
    filters.append("format=rgb24")
    if p.take is Take.VIDEO:
        filters.append(f"fade=t=in:st=0:d={VIDEO_FADE_IN_S}")
        filters.append(f"tpad=stop_mode=clone:stop_duration={TAIL_HOLD_S}")
        filters.append(f"fade=t=out:st={dur:.3f}:d={TAIL_HOLD_S}")
        filters.append(f"tpad=stop_mode=add:color=black:stop_duration={VIDEO_TAIL_BLACK_S}")
    else:
        filters.append(f"tpad=stop_mode=clone:stop_duration={TAIL_HOLD_S}")
        filters.append(f"tpad=stop_mode=add:color=black:stop_duration={GIF_TAIL_BLACK_S}")
    return ",".join(filters)


def cut_times(rec: Rec, video_end_e: float) -> tuple[float, float]:
    raw, work, tl = rec.capture.raw, rec.work.path, rec.timeline
    raw_s = media_seconds(raw)
    tl.flash_vt = find_flash(raw, work)
    tl.flash2_vt = find_flash(raw, work, max(0.0, raw_s - 8.0))
    rate = (tl.flash2_vt - tl.flash_vt) / (tl.flash2_e - tl.flash_e)
    log(f"sync flashes at {tl.flash_vt:.3f}s and {tl.flash2_vt:.3f}s, the capture at {rate:.4f} of wall time")
    if abs(rate - 1.0) > 0.05:
        raise RuntimeError("the 2 flashes don't line up with the wall clock")
    trim = tl.to_vt(tl.t0_e)
    return trim, tl.to_vt(video_end_e) - trim


def encode_video(rec: Rec, out_mp4: Path, video_end_e: float) -> Path:
    trim, dur = cut_times(rec, video_end_e)
    vf = vf_chain(rec, trim, dur)
    audio = build_audio(rec, trim, dur, random.Random(1))
    run(
        [
            "ffmpeg",
            "-v",
            "error",
            "-y",
            "-ss",
            f"{trim:.3f}",
            "-i",
            str(rec.capture.raw),
            "-i",
            str(audio),
            "-t",
            f"{dur + rec.profile.tail_s:.3f}",
            "-vf",
            vf,
            "-c:v",
            "libx265",
            "-preset",
            "slow",
            "-crf",
            "20",
            "-pix_fmt",
            "yuv420p",
            "-tag:v",
            "hvc1",
            "-x265-params",
            "log-level=error",
            "-r",
            str(rec.profile.out_fps),
            "-c:a",
            "aac",
            "-b:a",
            "160k",
            "-movflags",
            "+faststart",
            str(out_mp4),
        ]
    )
    return out_mp4


GIF_COLORS = 128


def encode_gif(rec: Rec, out_gif: Path, video_end_e: float) -> Path:
    trim, dur = cut_times(rec, video_end_e)
    vf = vf_chain(rec, trim, dur)
    raw = str(rec.capture.raw)
    pal = rec.work.path / "pal.png"
    cut = ["-ss", f"{trim:.3f}", "-t", f"{dur + rec.profile.tail_s:.3f}"]
    # one palette for the whole gif, with ordered dither, which stays still
    # from frame to frame where error diffusion shimmers
    run(
        [
            "ffmpeg",
            "-v",
            "error",
            "-y",
            *cut,
            "-i",
            raw,
            "-vf",
            f"{vf},palettegen=stats_mode=full:max_colors={GIF_COLORS}",
            str(pal),
        ]
    )
    run(
        [
            "ffmpeg",
            "-v",
            "error",
            "-y",
            *cut,
            "-i",
            raw,
            "-i",
            str(pal),
            "-lavfi",
            f"{vf}[x];[x][1:v]paletteuse=dither=bayer:bayer_scale=4",
            str(out_gif),
        ]
    )
    if not shutil.which("gifsicle"):
        log("WARNING: no gifsicle, so the gif isn't optimized")
        return out_gif
    opt = out_gif.with_name(out_gif.stem + "-opt.gif")
    run(["gifsicle", "-O3", "--no-warnings", "-o", str(opt), str(out_gif)])
    log(f"gifsicle: {out_gif.stat().st_size / (1 << 20):.1f} -> {opt.stat().st_size / (1 << 20):.1f} MiB")
    return opt


##•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
##	Where the output goes

GIF_ASSET_MAX_MB = 10  # GitHub stops animating one much past this
GIF_MAX_SECONDS = 40.0


def media_seconds(path: Path) -> float:
    text = out_of(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)]).strip()
    try:
        return float(text)
    except ValueError as e:
        raise RuntimeError(f"ffprobe gave no duration for {path}: {text!r}") from e


def rotate(out_dir: Path, prefix: str, ext: str) -> None:
    inc = REPO / "cicd/utility/include/gfs-rotate.bash"
    # not the include as $0, or it takes itself for run rather than sourced, and exits
    run(
        ["bash", "-c", 'source "$1" && gfs_rotate "$2" "$3" "$4"', "bash", str(inc), str(out_dir), prefix, ext],
        stdout=subprocess.DEVNULL,
    )


def place(src: Path, out_dir: Path, ext: str, no_rotate: bool) -> tuple[float, float]:
    out_dir.mkdir(parents=True, exist_ok=True)
    dst = out_dir / f"tukzmon-demo_{time.strftime('%Y%m%d-%H%M%S')}.{ext}"
    shutil.copy2(src, dst)
    mb, secs = dst.stat().st_size / (1 << 20), media_seconds(dst)
    if not no_rotate:
        rotate(out_dir, "tukzmon-demo", ext)
    log(f"{ext}: {dst} ({mb:.1f} MiB, {secs:.1f}s)")
    return mb, secs


def place_gif(gif: Path, opts: Options) -> None:
    mb, secs = place(gif, opts.out_dir / "gif", "gif", opts.no_rotate)
    scenes_s = secs - PROFILES[Take.GIF].tail_s
    if scenes_s > GIF_MAX_SECONDS:
        log(f"WARNING: the gif's scenes run {scenes_s:.1f}s, over {GIF_MAX_SECONDS:.0f}")
    asset = REPO / "assets" / "demo.gif"
    if opts.no_asset:
        log("assets/demo.gif: left alone (--no-asset)")
    elif mb > GIF_ASSET_MAX_MB:
        log(f"WARNING: the gif is {mb:.1f} MiB, over {GIF_ASSET_MAX_MB}, so assets/demo.gif is left alone")
    elif asset.exists() and asset.read_bytes() == gif.read_bytes():
        log("assets/demo.gif: no change")
    else:
        asset.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(gif, asset)
        log(f"assets/demo.gif: updated ({mb:.1f} MiB)")


##•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
##	Entry


def wanted(opts: Options) -> list[tuple[str, Scene]]:
    return [(name, fn) for name, fn in SCENES.items() if not opts.scenes or name in opts.scenes]


def take_shot(opts: Options, base: Path) -> None:
    if opts.shot is None:
        raise ValueError("take_shot needs --shot")
    rec = Rec(opts, PROFILES[opts.takes[0]], base)
    try:
        rec.start()
        m = Mouse(rec)
        scenes = wanted(opts)
        if scenes and scenes[0][0] != "fresh":
            rec.panel.start()
            rec.panel.wait()
            time.sleep(2.0)
        for name, fn in scenes:
            log(f"scene: {name}")
            fn(rec, m)
        time.sleep(0.4)
        rec.screen.shot(opts.shot)
        if rec.settings:
            log(f"settings window corner {rec.settings.origin}")
    finally:
        rec.cleanup()


def record(opts: Options, take: Take, base: Path) -> None:
    rec = Rec(opts, PROFILES[take], base)
    tl = rec.timeline
    try:
        rec.start()
        rec.capture.start()
        # the lead is a still desktop, held here, so the cut starts clear of the flash
        tl.t0_e = time.time()
        time.sleep(LEAD_S)
        m = Mouse(rec)
        for name, fn in wanted(opts):
            began = time.time()
            fn(rec, m)
            log(f"[{take.value}] scene: {name}, {time.time() - began:.1f}s")
        time.sleep(0.3)
        video_end_e = time.time()
        # the last caption stays up through the held frame
        if tl.captions:
            tl.captions[-1].end += TAIL_HOLD_S
        time.sleep(0.5)
        tl.flash2_e = rec.capture.flash()
        time.sleep(1.0)
        rec.capture.stop()
        rec.panel.stop()
        if take is Take.VIDEO:
            out = encode_video(rec, rec.work.path / "demo.mp4", video_end_e)
            place(out, opts.out_dir / "video", "mp4", opts.no_rotate)
        else:
            gif = encode_gif(rec, rec.work.path / "demo.gif", video_end_e)
            place_gif(gif, opts)
        if rec.keep:
            log(f"[{take.value}] work folder kept: {rec.work.path}")
    finally:
        rec.cleanup()


def comma_list(text: str) -> list[str]:
    return [s.strip() for s in text.split(",") if s.strip()]


def parse_args() -> Options:
    ap = argparse.ArgumentParser(description="Record the tukzmon demo video and gif.")
    ap.add_argument("--display", default="", help=f"pin a display; by default a search from {DEFAULT_DISPLAY} down")
    ap.add_argument("--profile", default="video,gif", help="comma list: video,gif")
    ap.add_argument("--scenes", default="", help="comma list; all by default")
    ap.add_argument("--lib-dir", default=str(REPO / "target/release"))
    ap.add_argument("--history-bin", default=str(REPO / "target/release/examples/demo-history"))
    ap.add_argument("--keep-work", action="store_true")
    ap.add_argument("--no-rotate", action="store_true")
    ap.add_argument("--no-asset", action="store_true", help="leave assets/demo.gif alone")
    ap.add_argument("--out-dir", default=str(OUT_DIR))
    ap.add_argument("--shot", default="", help="run the scenes with no capture, write one screenshot, and stop")
    args = ap.parse_args()

    lib_dir, history_bin = Path(args.lib_dir), Path(args.history_bin)
    for f in ("libtukzmon.so", "tukzmon-settings"):
        if not (lib_dir / f).is_file():
            ap.exit(1, f"no {f} in {lib_dir}: cargo build --release -p tukzmon-xfce -p tukzmon-settings\n")
    if not history_bin.is_file():
        ap.exit(1, f"no {history_bin}: cargo build --release -p tukzmon-core --example demo-history\n")
    scenes = comma_list(args.scenes)
    for name in scenes:
        if name not in SCENES:
            ap.exit(1, f"no scene '{name}'. Scenes: {', '.join(SCENES)}\n")
    known = {take.value: take for take in Take}
    takes = comma_list(args.profile)
    for name in takes:
        if name not in known:
            ap.exit(1, f"no profile '{name}'\n")
    pinned = args.display or os.environ.get("TUKZMON_DEMO_DISPLAY") or ""
    return Options(
        display=pick_display(pinned or DEFAULT_DISPLAY, bool(pinned)),
        takes=[known[name] for name in takes],
        scenes=scenes,
        lib_dir=lib_dir,
        history_bin=history_bin,
        keep_work=args.keep_work,
        no_rotate=args.no_rotate,
        no_asset=args.no_asset,
        out_dir=Path(args.out_dir),
        shot=Path(args.shot) if args.shot else None,
    )


def main() -> None:
    opts = parse_args()
    base, made = temp_root()
    (base / ".demo-run").touch()
    log(f"display {opts.display}, work in {base}")
    try:
        if opts.shot:
            take_shot(opts, base)
            return
        for take in opts.takes:
            record(opts, take, base)
    finally:
        if made and not opts.keep_work:
            safe_rmtree(base)


if __name__ == "__main__":
    main()


##	History:
##		- 2026-11-06: Created, from nemo-anywhere's recorder, with the panel, the
##		  days of history from core/examples/demo-history.rs, and the settings
##		  window's Graphs tab.
##		- 2026-11-06: The panel has the usual items, with tukzmon at the right,
##		  and the scenes' panel points are from wherever it lands.
##		- 2026-11-07: Type hints, dataclasses and enums, and the recorder split
##		  into the screen, panel, settings window and capture. A failed probe or
##		  rotation stops the run. The pointer moves through XTEST, the caption
##		  band's strut is on the same connection, and the panel stop waits on
##		  pidfds after one scan.
