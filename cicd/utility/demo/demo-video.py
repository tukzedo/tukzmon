#!/usr/bin/env python3

##	Purpose:
##		Records tukzmon's demo video and README gif, per script.txt beside this.
##		A real xfce4-panel with the release plugin runs on a private Xvfb (never
##		:0), under xfwm4, with a throwaway HOME. The mouse is driven with xdotool,
##		clicks get foley synced to the input times, and captions sit in a band
##		under the desktop. 2 takes from one scene list:
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
##	History: at bottom.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT

import argparse
import contextlib
import os
import random
import re
import shutil
import signal
import subprocess
import sys
import time
import wave
from pathlib import Path

import numpy as np

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
TAIL_EXTRA = {"gif": TAIL_HOLD_S + GIF_TAIL_BLACK_S, "video": TAIL_HOLD_S + VIDEO_TAIL_BLACK_S}
FOLEY_LAG = 0.01

# gif px; the video doubles all of it
VIEW_W, VIEW_H = 960, 540
BAND = 46  # captions, at the bottom
PANEL = 56  # panel height
DESK = "#1d2430"  # root color, the band's too

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

BAND_STRUT = """
import sys, time
from Xlib import X, display
disp = display.Display()
scr = disp.screen()
width, height, band = int(sys.argv[1]), int(sys.argv[2]), int(sys.argv[3])
win = scr.root.create_window(0, height - 1, 1, 1, 0, scr.root_depth, X.InputOutput,
    X.CopyFromParent, background_pixel=scr.black_pixel)
atom = disp.intern_atom
win.change_property(atom("_NET_WM_WINDOW_TYPE"), atom("ATOM"), 32, [atom("_NET_WM_WINDOW_TYPE_DOCK")])
win.change_property(atom("_NET_WM_STRUT"), atom("CARDINAL"), 32, [0, 0, 0, band])
win.change_property(atom("_NET_WM_STRUT_PARTIAL"), atom("CARDINAL"), 32,
    [0, 0, 0, band, 0, 0, 0, 0, 0, 0, 0, width - 1])
win.map()
disp.sync()
while True:
    time.sleep(3600)
"""

PROFILES = {
    "video": dict(scale=2, cap_fps=60, out_fps=60, banner_fs=34, audio=True, banner_min=3.0),
    "gif": dict(scale=1, cap_fps=50, out_fps=50, banner_fs=20, audio=False, banner_min=2.5),
}


def log(msg):
    print(f"[demo] {msg}", flush=True)


def run(cmd, **kw):
    return subprocess.run(cmd, check=True, **kw)


def out_of(cmd, **kw):
    return subprocess.run(cmd, check=True, capture_output=True, text=True, **kw).stdout


def display_taken(num):
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


def pick_display(wanted, explicit):
    if explicit or not display_taken(wanted.lstrip(":")):
        return wanted
    base = int(wanted.lstrip(":"))
    for num in range(base - 1, base - DISPLAY_TRIES, -1):
        if not display_taken(num):
            log(f"display {wanted} is taken; using :{num}")
            return f":{num}"
    raise RuntimeError(f"no free display from {wanted} down {DISPLAY_TRIES}")


def temp_root():
    """The run folder: cicd's when it set TMPDIR to one, else a new one."""
    host = Path(os.environ.get("HOST_TEMP") or os.environ.get("TMPDIR") or "/tmp").resolve()
    proj = host / "tukzmon"
    tmp = Path(os.environ.get("TMPDIR", "")).resolve() if os.environ.get("TMPDIR") else None
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


def safe_rmtree(path):
    # Only a folder this run made: real, under <temp>/tukzmon/, below the
    # project folder itself, and holding the run's marker file.
    p = Path(path)
    host = Path(os.environ.get("HOST_TEMP") or os.environ.get("TMPDIR") or "/tmp").resolve()
    proj = host / "tukzmon"
    if p.is_symlink() or not p.is_dir():
        log(f"WARNING: not removing {p}: not a plain folder")
        return
    real = p.resolve()
    if not real.is_relative_to(proj) or real == proj or "tmp" not in real.parts:
        log(f"WARNING: not removing {p}: outside {proj}")
        return
    if not (real / ".demo-run").is_file():
        log(f"WARNING: not removing {p}: no run marker")
        return
    shutil.rmtree(real)


class Rec:
    def __init__(self, args, profile, base):
        self.p = profile
        self.scale = profile["scale"]
        self.size = (VIEW_W * self.scale, VIEW_H * self.scale)
        self.band = BAND * self.scale
        self.cap_fps = profile["cap_fps"]
        self.out_fps = profile["out_fps"]
        self.display = args.display
        self.num = self.display.lstrip(":")
        self.lib_dir = Path(args.lib_dir).resolve()
        self.history_bin = Path(args.history_bin).resolve()
        self.work = base / f"take-{self.scale}x"
        self.work.mkdir(parents=True, exist_ok=True)
        (self.work / ".demo-run").touch()
        self.home = self.work / "home"
        self.prefix = self.work / "prefix"
        self.keep = args.keep_work
        self.events = []  # (epoch, kind)
        self.banners = []  # (epoch_start, epoch_end, text)
        self.xvfb = None
        self.wm = None
        self.panel = None
        self.strut = None
        self.ff = None
        self.flash_e = 0.0
        self.flash2_e = 0.0
        self.t0_e = 0.0
        self.settings_origin = (0, 0)
        self.logs = []
        # --shot also saves a picture at each peek, beside the shot
        self.peeks = Path(args.shot).with_suffix("") if args.shot else None

    # --- environment --------------------------------------------------------
    def env(self):
        e = dict(os.environ)
        e.update(DISPLAY=self.display)
        e[MARK] = str(self.work)
        # with these set a GTK or Qt app would open on the real desktop
        for k in ("WAYLAND_DISPLAY", "XDG_SESSION_TYPE", "DBUS_SESSION_BUS_ADDRESS", "XAUTHORITY"):
            e.pop(k, None)
        return e

    def desk_env(self):
        # the panel, the plugin, and the settings window the plugin starts
        e = self.env()
        e[DESK_MARK] = str(self.work)
        h = Path(DEMO_HOME)
        e.update(
            HOME=str(h),
            XDG_CONFIG_HOME=str(h / ".config"),
            XDG_DATA_HOME=str(h / ".local/share"),
            XDG_CACHE_HOME=str(h / ".cache"),
            XDG_STATE_HOME=str(h / ".local/state"),
            XDG_RUNTIME_DIR=str(h / "run"),
            XDG_DATA_DIRS=f"{DEMO_PREFIX}/share:/usr/share",
            GDK_SCALE=str(self.scale),
            GDK_BACKEND="x11",
            GTK_THEME="Adwaita:dark",
            QT_QPA_PLATFORM="xcb",
            QT_QPA_PLATFORMTHEME="gtk3",
            QT_SCALE_FACTOR=str(self.scale),
            QT_ENABLE_HIGHDPI_SCALING="0",
            NO_AT_BRIDGE="1",
        )
        for k in ("GDK_DPI_SCALE", "QT_AUTO_SCREEN_SCALE_FACTOR", "QT_SCREEN_SCALE_FACTORS", "QT_FONT_DPI"):
            e.pop(k, None)
        return e

    def xdo(self, *a):
        subprocess.run(
            ["xdotool", *a], env=self.env(), check=False, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL
        )

    def px(self, lx, ly):
        return int(lx * self.scale), int(ly * self.scale)

    # --- the throwaway home -------------------------------------------------
    def prep_home(self):
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
            d = self.prefix / sub
            d.mkdir(parents=True, exist_ok=True)
            for f in ("libtukzmon.so", "tukzmon-settings"):
                shutil.copy2(self.lib_dir / f, d)
        (h / ".config/gtk-3.0/settings.ini").write_text(
            "[Settings]\ngtk-theme-name=Adwaita\ngtk-application-prefer-dark-theme=1\n"
            "gtk-font-name=Lato 10\ngtk-enable-animations=0\n",
            encoding="utf-8",
        )
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
      <property name="plugin-ids" type="array">
        <value type="int" value="1"/>
      </property>
    </property>
  </property>
  <property name="plugins" type="empty">
    <property name="plugin-1" type="string" value="tukzmon"/>
  </property>
</channel>
"""
        (h / ".config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml").write_text(panel, encoding="utf-8")

    @property
    def hist(self):
        return self.home / ".local/share/tukzmon/plugin-1.hist"

    # --- display, WM, band --------------------------------------------------
    def make_theme(self, wmhome):
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

    def start_display(self):
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
            stderr=self.log_to("xvfb.log", "w"),
            start_new_session=True,
        )
        deadline = time.time() + 10
        while time.time() < deadline:
            lock = Path(f"/tmp/.X{self.num}-lock")
            if lock.exists() and lock.read_text(encoding="utf-8").strip() == str(self.xvfb.pid):
                break
            time.sleep(0.1)
        else:
            raise RuntimeError(f"Xvfb on {self.display} never came up as ours (see xvfb.log)")
        time.sleep(0.5)
        run(["xdpyinfo"], env=self.env(), stdout=subprocess.DEVNULL)
        self.start_wm()
        time.sleep(1.5)
        self.reserve_band()
        # after the WM, which paints the root black as it starts
        subprocess.run(["xsetroot", "-solid", DESK], env=self.env(), check=False)

    def start_wm(self):
        wmhome = self.work / "wmhome"
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
            stdout=self.log_to("wm.log", "w"),
            stderr=subprocess.STDOUT,
            start_new_session=True,
        )

    def reserve_band(self):
        # a strut, so the WM keeps windows out of the caption band
        self.strut = subprocess.Popen(
            [sys.executable, "-c", BAND_STRUT, str(self.size[0]), str(self.size[1]), str(self.band)],
            env=self.env(),
            stdout=subprocess.DEVNULL,
            stderr=self.log_to("strut.log", "w"),
            start_new_session=True,
        )
        time.sleep(0.6)
        if self.strut.poll() is not None:
            log("WARNING: couldn't reserve the caption band (see strut.log)")

    # --- the panel ----------------------------------------------------------
    def sandbox(self):
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

    def start_panel(self):
        self.panel = subprocess.Popen(
            [*self.sandbox(), "dbus-run-session", "--", "xfce4-panel"],
            env=self.desk_env(),
            stdout=self.log_to("panel.log", "a"),
            stderr=subprocess.STDOUT,
            stdin=subprocess.DEVNULL,
            start_new_session=True,
        )

    def wait_panel(self, timeout=20):
        deadline = time.time() + timeout
        while time.time() < deadline:
            r = subprocess.run(
                ["xdotool", "search", "--onlyvisible", "--class", "xfce4-panel"],
                env=self.env(),
                capture_output=True,
                text=True,
            )
            if r.stdout.split() and self.plugin_pids():
                return
            time.sleep(0.2)
        raise RuntimeError("the panel never showed (see panel.log)")

    def plugin_pids(self):
        return [p for p, args in self.ours() if "wrapper-2.0" in args and "libtukzmon" in args]

    def stop_panel(self):
        # the plugin saves its history on the way out, so wait for it to be gone
        # before anything writes that file
        if self.panel:
            kill_group(self.panel)
            self.panel = None
        deadline = time.time() + 5
        while time.time() < deadline and any("wrapper-2.0" in a for _, a in self.ours(desk=True)):
            time.sleep(0.05)
        # the bus's helpers, such as xfconfd, outlive the group
        for pid, _ in self.ours(desk=True):
            stop_pid(pid)

    def make_history(self, days=4.0):
        # written while the panel still runs, so it's down only for the copy
        self.days = self.work / "days.hist"
        run([str(self.history_bin), str(self.days), str(days)], env=self.env())

    def put_history(self):
        self.hist.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(self.days, self.hist)

    # --- the settings window ------------------------------------------------
    def settings_window(self, timeout=20):
        deadline = time.time() + timeout
        while time.time() < deadline:
            r = subprocess.run(
                ["xdotool", "search", "--onlyvisible", "--class", "tukzmon-settings"],
                env=self.env(),
                capture_output=True,
                text=True,
            )
            for win in r.stdout.split():
                info = out_of(["xwininfo", "-id", win], env=self.env())
                m = re.search(r"Width:\s*(\d+)", info)
                if m and int(m.group(1)) > 200:
                    return win
            time.sleep(0.2)
        raise RuntimeError("the settings window never showed")

    def client_geom(self, win):
        info = out_of(["xwininfo", "-id", win], env=self.env())

        def num(k):
            return int(re.search(rf"{k}:\s*(-?\d+)", info).group(1))

        return num("Absolute upper-left X"), num("Absolute upper-left Y"), num("Width"), num("Height")

    def frame_extents(self, win):
        r = subprocess.run(["xprop", "-id", win, "_NET_FRAME_EXTENTS"], env=self.env(), capture_output=True, text=True)
        m = re.search(r"=\s*(\d+),\s*(\d+),\s*(\d+),\s*(\d+)", r.stdout)
        return tuple(map(int, m.groups())) if m else (2, 2, 32, 2)

    def place_settings(self, win):
        # centered in the room between the panel and the band
        left, right, top, bottom = self.frame_extents(win)
        _, _, w, h = self.client_geom(win)
        room_t = PANEL * self.scale
        room_h = self.size[1] - room_t - self.band
        want_x = max(0, (self.size[0] - (w + left + right)) // 2) + left
        want_y = room_t + max(0, (room_h - (h + top + bottom)) // 2) + top
        target = [want_x, want_y]
        for _ in range(4):
            self.xdo("windowmove", win, str(target[0] - left), str(target[1] - top))
            time.sleep(0.25)
            cx, cy, _, _ = self.client_geom(win)
            dx, dy = want_x - cx, want_y - cy
            if abs(dx) <= 1 and abs(dy) <= 1:
                break
            target[0] += dx
            target[1] += dy
        cx, cy, _, _ = self.client_geom(win)
        self.settings_origin = (cx, cy)

    def sp(self, lx, ly):
        """A point in the settings window, in gif px from its client corner."""
        return self.settings_origin[0] + int(lx * self.scale), self.settings_origin[1] + int(ly * self.scale)

    # --- capture ------------------------------------------------------------
    def start_capture(self):
        self.raw = self.work / "raw.mkv"
        self.ff = subprocess.Popen(
            [
                "ffmpeg",
                "-hide_banner",
                "-loglevel",
                "error",
                "-progress",
                str(self.work / "ffprogress.txt"),
                "-f",
                "x11grab",
                "-draw_mouse",
                "1",
                "-framerate",
                str(self.cap_fps),
                "-video_size",
                f"{self.size[0]}x{self.size[1]}",
                "-i",
                self.display,
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
            env=self.env(),
            stdin=subprocess.DEVNULL,
            stderr=self.log_to("ffmpeg.log", "w"),
        )
        prog = self.work / "ffprogress.txt"
        deadline = time.time() + 30
        while time.time() < deadline:
            if prog.exists() and re.search(r"(?m)^frame=([1-9]\d*)", prog.read_text(encoding="utf-8")):
                break
            time.sleep(0.3)
        else:
            raise RuntimeError("x11grab gave no frames (see ffmpeg.log)")
        time.sleep(0.8)
        self.flash_e = self.flash()
        self.mouse_park()
        time.sleep(0.4)

    def flash(self):
        subprocess.run(["xsetroot", "-solid", "white"], env=self.env(), check=False)
        at = time.time()
        time.sleep(0.25)
        subprocess.run(["xsetroot", "-solid", DESK], env=self.env(), check=False)
        return at

    def to_vt(self, epoch):
        """Wall clock to the capture's time, between the 2 flashes, since a
        busy box can run the capture a percent or so slow."""
        rate = (self.flash2_vt - self.flash_vt) / (self.flash2_e - self.flash_e)
        return self.flash_vt + (epoch - self.flash_e) * rate

    def stop_capture(self):
        if self.ff:
            self.ff.send_signal(signal.SIGINT)
            try:
                self.ff.wait(timeout=30)
            except subprocess.TimeoutExpired:
                self.ff.kill()
            self.ff = None

    def shot(self, path):
        run(["import", "-window", "root", str(path)], env=self.env())
        log(f"screenshot: {path}")

    def log_to(self, name, how="w"):
        # Popen keeps writing after this returns, so cleanup closes it
        f = open(self.work / name, how, encoding="utf-8")  # noqa: SIM115
        self.logs.append(f)
        return f

    def peek(self, name):
        if self.peeks:
            self.shot(Path(f"{self.peeks}-{name}.png"))

    def ev(self, kind):
        self.events.append((time.time(), kind))

    def mouse_park(self):
        # the arrow's tip is its hotspot, so in the corner none of it shows
        self.xdo("mousemove", str(self.size[0] - 1), str(self.size[1] - 1))

    def ours(self, desk=False):
        """(pid, cmdline) of every live process with this run's mark, or with
        the panel side's mark too."""
        want = f"{DESK_MARK if desk else MARK}={self.work}".encode()
        found = []
        for d in Path("/proc").iterdir():
            if not d.name.isdigit() or int(d.name) == os.getpid():
                continue
            try:
                if want in (d / "environ").read_bytes().split(b"\0"):
                    found.append(
                        (int(d.name), (d / "cmdline").read_bytes().replace(b"\0", b" ").decode(errors="replace"))
                    )
            except OSError:
                continue
        return found

    def cleanup(self):
        self.stop_capture()
        self.stop_panel()
        for proc in (self.strut, self.wm):
            if proc:
                kill_group(proc)
        for pid, _ in self.ours():
            stop_pid(pid)
        if self.xvfb:
            kill_group(self.xvfb)
            self.xvfb = None
        for f in self.logs:
            f.close()
        if not self.keep and self.work.exists():
            safe_rmtree(self.work)


def kill_group(proc):
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


def stop_pid(pid):
    try:
        os.kill(pid, signal.SIGTERM)
    except ProcessLookupError:
        return
    deadline = time.time() + 3
    while time.time() < deadline:
        try:
            os.kill(pid, 0)
        except ProcessLookupError:
            return
        time.sleep(0.05)
    with contextlib.suppress(ProcessLookupError):
        os.kill(pid, signal.SIGKILL)


##•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
##	Mouse and captions


class Mouse:
    def __init__(self, rec):
        self.rec = rec
        self.pos = (rec.size[0] - 1, rec.size[1] - 1)

    def move(self, lx, ly, dur=0.45):
        self.move_px(*self.rec.px(lx, ly), dur)

    def move_px(self, x, y, dur=0.45):
        x0, y0 = self.pos
        steps = max(6, int(dur * 50))
        began = time.time()
        for i in range(1, steps + 1):
            t = i / steps
            t = t * t * (3 - 2 * t)
            self.rec.xdo("mousemove", str(int(x0 + (x - x0) * t)), str(int(y0 + (y - y0) * t)))
            # by the clock, since each xdotool call takes a while too
            time.sleep(max(0.0, began + dur * i / steps - time.time()))
        self.pos = (x, y)

    def click(self, button="1", quiet=False):
        # logged first, since the press goes out at the start of the call
        self.rec.ev("mouse:CLICK_Q" if quiet else "mouse:CLICK")
        self.rec.xdo("click", button)

    def at(self, lx, ly, dur=0.45, settle=0.45, button="1"):
        self.move(lx, ly, dur)
        time.sleep(0.12)
        self.click(button)
        time.sleep(settle)

    def at_win(self, lx, ly, dur=0.45, settle=0.45):
        self.move_px(*self.rec.sp(lx, ly), dur)
        time.sleep(0.12)
        self.click()
        time.sleep(settle)

    def win(self, lx, ly, dur=0.45):
        self.move_px(*self.rec.sp(lx, ly), dur)


class Banner:
    def __init__(self, rec, text):
        self.rec, self.text = rec, text

    def __enter__(self):
        self.start = time.time()
        return self

    def __exit__(self, *exc):
        self.rec.banners.append((self.start, time.time(), self.text))


##•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
##	Scenes, in gif px. script.txt says what each one shows.

# the graphs, left to right, at the defaults and a 56 px panel
CPU_X = (2, 102)
GRAPH_Y = 28
TIP_STOPS = (96, 40, 3)  # across the CPU graph, newest to oldest

PROPS_ITEM = (118, 100)  # the item menu's Properties, with the menu opened at (60, GRAPH_Y)
GRAPHS_TAB = (35, 60)  # in the settings window
NET_SHOW = (121, 209)  # the network row's show box
THEMES_TAB = (37, 92)
THEME_PICK = (205, 37)  # the theme name dropdown
NEON = (189, 110)  # in its open list
APPLY_BUTTON = (-140, -28)  # from the window's bottom right
OK_BUTTON = (-53, -28)  # from the window's bottom right


def scene_fresh(r, m):
    r.start_panel()
    r.wait_panel()
    with Banner(r, "tukzmon: system graphs for the XFCE panel"):
        time.sleep(2.6)
    with Banner(r, "From a fresh start, the graphs fill in and settle"):
        time.sleep(4.0)


def scene_days(r, m):
    with Banner(r, "A few days later, the log time axis still fits them all"):
        r.make_history()
        r.stop_panel()
        r.put_history()
        r.start_panel()
        r.wait_panel()
        time.sleep(2.2)
    with Banner(r, "Hover for the value, and how long ago"):
        m.move(TIP_STOPS[0], GRAPH_Y, 0.6)
        time.sleep(1.5)
        r.peek(f"tip{TIP_STOPS[0]}")
        for x in TIP_STOPS[1:]:
            m.move(x, GRAPH_Y, 0.5)
            time.sleep(1.4)
            r.peek(f"tip{x}")


def scene_settings(r, m):
    with Banner(r, "Right-click, Properties"):
        m.at(60, GRAPH_Y, button="3", settle=0.8)
        r.peek("menu")
        m.at(*PROPS_ITEM, settle=0.3)
        win = r.settings_window()
        r.place_settings(win)
        time.sleep(0.8)
        r.peek("open")
    _, _, w, h = r.client_geom(win)

    def corner(x, y):
        return (w / r.scale + x, h / r.scale + y)

    with Banner(r, "The Graphs tab: which graphs, in what order, how wide"):
        m.at_win(*GRAPHS_TAB, settle=1.6)
        r.peek("graphs")
        m.at_win(*NET_SHOW, settle=0.5)
        m.at_win(*corner(*APPLY_BUTTON), settle=1.6)
        r.peek("net")
    with Banner(r, "One theme colors every graph"):
        m.at_win(*THEMES_TAB, settle=0.7)
        m.at_win(*THEME_PICK, settle=0.6)
        r.peek("list")
        m.at_win(*NEON, settle=0.4)
        m.at_win(*corner(*APPLY_BUTTON), settle=1.8)
        r.peek("neon")
        m.at_win(*corner(*OK_BUTTON), settle=1.0)


def scene_outro(r, m):
    with Banner(r, "github.com/tukzedo/tukzmon"):
        m.move(VIEW_W - 120, 300, 0.5)
        time.sleep(1.2)


SCENES = [
    ("fresh", scene_fresh),
    ("days", scene_days),
    ("settings", scene_settings),
    ("outro", scene_outro),
]


##•••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••••
##	Audio: the clicks, mixed from the event log

SOUND_FILES = {
    "mouse:CLICK": SOUNDS / "mouse/click.wav",
    "mouse:CLICK_Q": SOUNDS / "mouse/click_quiet.wav",
}
GAIN = {"mouse:CLICK": 0.55, "mouse:CLICK_Q": 0.38}


def load_samples(work):
    cache = {}
    for kind, path in SOUND_FILES.items():
        wav = work / (re.sub(r"[^A-Za-z0-9]", "_", kind) + ".wav")
        run(["ffmpeg", "-v", "error", "-y", "-i", str(path), "-ar", str(SR), "-ac", "2", "-f", "wav", str(wav)])
        with wave.open(str(wav), "rb") as w:
            data = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16)
        pcm = data.astype(np.float32).reshape(-1, 2) / 32768.0
        # the files have about 0.16 s of quiet before the click itself
        loud = np.abs(pcm).max(axis=1)
        onset = int(np.argmax(loud > 0.2 * loud.max()))
        cache[kind] = pcm[max(0, onset - SR // 500) :]
    return cache


def build_audio(rec, work, trim, duration, rng):
    cache = load_samples(work)
    mix = np.zeros((int(duration * SR) + SR, 2), dtype=np.float32)
    for epoch, kind in rec.events:
        t_rel = rec.to_vt(epoch) - trim + FOLEY_LAG
        s = cache.get(kind)
        if s is None or t_rel < -0.5 or t_rel > duration:
            continue
        gain = GAIN.get(kind, 0.5) * rng.uniform(0.85, 1.05)
        at = int(max(0.0, t_rel) * SR)
        end = min(at + len(s), len(mix))
        mix[at:end] += s[: end - at] * gain
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


def find_flash(raw, work, start=0.0):
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


def vf_chain(rec, work, trim, dur, tail=None):
    p = rec.p

    to_vt = rec.to_vt

    # the capture runs on past the scenes to the end flash, and the tail is padded on after this
    filters = [f"trim=end={dur:.3f}", f"fps={rec.out_fps}"]
    spans = []
    for s_e, e_e, text in rec.banners:
        s = max(0.0, to_vt(s_e) - trim)
        e = max(s + p["banner_min"], to_vt(e_e) - trim)
        spans.append([s, e, text])
    spans.sort(key=lambda b: b[0])
    # one caption at a time
    for i in range(len(spans) - 1):
        spans[i][1] = min(spans[i][1], spans[i + 1][0] - 0.3)
    y = f"h-{rec.band}+({rec.band}-text_h)/2"
    for i, (s, e, text) in enumerate(spans):
        if e <= s:
            continue
        tf = work / f"banner{i}.txt"
        tf.write_text(text, encoding="utf-8")
        fade = f"clip((t-{s:.3f})/0.15,0,1)*clip(({e:.3f}-t)/0.15,0,1)"
        filters.append(
            f"drawtext=fontfile={BANNER_TTF}:textfile={tf}:fontsize={p['banner_fs']}:"
            f"fontcolor={BANNER_FG}:x=(w-text_w)/2:y={y}:alpha='{fade}':enable='between(t,{s:.3f},{e:.3f})'"
        )
    filters.append("format=rgb24")
    if tail == "video":
        filters.append(f"fade=t=in:st=0:d={VIDEO_FADE_IN_S}")
        filters.append(f"tpad=stop_mode=clone:stop_duration={TAIL_HOLD_S}")
        filters.append(f"fade=t=out:st={dur:.3f}:d={TAIL_HOLD_S}")
        filters.append(f"tpad=stop_mode=add:color=black:stop_duration={VIDEO_TAIL_BLACK_S}")
    elif tail == "gif":
        filters.append(f"tpad=stop_mode=clone:stop_duration={TAIL_HOLD_S}")
        filters.append(f"tpad=stop_mode=add:color=black:stop_duration={GIF_TAIL_BLACK_S}")
    return ",".join(filters)


def cut_times(rec, video_end_e):
    raw_s = media_seconds(rec.raw)
    rec.flash_vt = find_flash(rec.raw, rec.work)
    rec.flash2_vt = find_flash(rec.raw, rec.work, max(0.0, raw_s - 8.0))
    rate = (rec.flash2_vt - rec.flash_vt) / (rec.flash2_e - rec.flash_e)
    log(f"sync flashes at {rec.flash_vt:.3f}s and {rec.flash2_vt:.3f}s, the capture at {rate:.4f} of wall time")
    if abs(rate - 1.0) > 0.05:
        raise RuntimeError("the 2 flashes don't line up with the wall clock")
    trim = rec.to_vt(rec.t0_e)
    return trim, rec.to_vt(video_end_e) - trim


def encode_video(rec, out_mp4, video_end_e):
    trim, dur = cut_times(rec, video_end_e)
    vf = vf_chain(rec, rec.work, trim, dur, tail="video")
    audio = build_audio(rec, rec.work, trim, dur, random.Random(1))
    run(
        [
            "ffmpeg",
            "-v",
            "error",
            "-y",
            "-ss",
            f"{trim:.3f}",
            "-i",
            str(rec.raw),
            "-i",
            str(audio),
            "-t",
            f"{dur + TAIL_EXTRA['video']:.3f}",
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
            str(rec.out_fps),
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


def encode_gif(rec, out_gif, video_end_e):
    trim, dur = cut_times(rec, video_end_e)
    vf = vf_chain(rec, rec.work, trim, dur, tail="gif")
    pal = rec.work / "pal.png"
    cut = ["-ss", f"{trim:.3f}", "-t", f"{dur + TAIL_EXTRA['gif']:.3f}"]
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
            str(rec.raw),
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
            str(rec.raw),
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


def media_seconds(path):
    try:
        return float(
            out_of(["ffprobe", "-v", "error", "-show_entries", "format=duration", "-of", "csv=p=0", str(path)]).strip()
        )
    except (subprocess.CalledProcessError, ValueError):
        return 0.0


def rotate(out_dir, prefix, ext, no_rotate):
    if no_rotate:
        return
    inc = REPO / "cicd/utility/include/gfs-rotate.bash"
    subprocess.run(
        ["bash", "-c", 'source "$0" && gfs_rotate "$1" "$2" "$3"', str(inc), str(out_dir), prefix, ext],
        check=False,
        stdout=subprocess.DEVNULL,
    )


def place(src, out_dir, ext, no_rotate):
    out_dir.mkdir(parents=True, exist_ok=True)
    dst = out_dir / f"tukzmon-demo_{time.strftime('%Y%m%d-%H%M%S')}.{ext}"
    shutil.copy2(src, dst)
    mb, secs = dst.stat().st_size / (1 << 20), media_seconds(dst)
    rotate(out_dir, "tukzmon-demo", ext, no_rotate)
    log(f"{ext}: {dst} ({mb:.1f} MiB, {secs:.1f}s)")
    return mb, secs


def place_gif(gif, out_dir, no_rotate, no_asset):
    mb, secs = place(gif, out_dir, "gif", no_rotate)
    if secs - TAIL_EXTRA["gif"] > GIF_MAX_SECONDS:
        log(f"WARNING: the gif's scenes run {secs - TAIL_EXTRA['gif']:.1f}s, over {GIF_MAX_SECONDS:.0f}")
    asset = REPO / "assets" / "demo.gif"
    if no_asset:
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


def wanted(args):
    names = [s.strip() for s in args.scenes.split(",") if s.strip()]
    for n in names:
        if n not in dict(SCENES):
            sys.exit(f"no scene '{n}'. Scenes: {', '.join(n for n, _ in SCENES)}")
    return [(n, fn) for n, fn in SCENES if not names or n in names]


def take_shot(args, name, base):
    rec = Rec(args, PROFILES[name], base)
    try:
        rec.prep_home()
        rec.start_display()
        m = Mouse(rec)
        scenes = wanted(args)
        if scenes and scenes[0][0] != "fresh":
            rec.start_panel()
            rec.wait_panel()
            time.sleep(2.0)
        for seg, fn in scenes:
            log(f"scene: {seg}")
            fn(rec, m)
        time.sleep(0.4)
        rec.shot(Path(args.shot))
        log(f"settings window corner {rec.settings_origin}")
    finally:
        rec.cleanup()


def record(args, name, base):
    rec = Rec(args, PROFILES[name], base)
    try:
        rec.prep_home()
        rec.start_display()
        rec.start_capture()
        # the lead is a still desktop, held here, so the cut starts clear of the flash
        rec.t0_e = time.time()
        time.sleep(LEAD_S)
        m = Mouse(rec)
        for seg, fn in wanted(args):
            began = time.time()
            fn(rec, m)
            log(f"[{name}] scene: {seg}, {time.time() - began:.1f}s")
        time.sleep(0.3)
        video_end_e = time.time()
        # the last caption stays up through the held frame
        if rec.banners:
            last = rec.banners[-1]
            rec.banners[-1] = (last[0], last[1] + TAIL_HOLD_S, last[2])
        time.sleep(0.5)
        rec.flash2_e = rec.flash()
        time.sleep(1.0)
        rec.stop_capture()
        rec.stop_panel()
        out_dir = Path(args.out_dir)
        if name == "video":
            out = encode_video(rec, rec.work / "demo.mp4", video_end_e)
            place(out, out_dir / "video", "mp4", args.no_rotate)
        else:
            gif = encode_gif(rec, rec.work / "demo.gif", video_end_e)
            place_gif(gif, out_dir / "gif", args.no_rotate, args.no_asset)
        if rec.keep:
            log(f"[{name}] work folder kept: {rec.work}")
    finally:
        rec.cleanup()


def main():
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

    for f in ("libtukzmon.so", "tukzmon-settings"):
        if not (Path(args.lib_dir) / f).is_file():
            sys.exit(f"no {f} in {args.lib_dir}: cargo build --release -p tukzmon-xfce -p tukzmon-settings")
    if not Path(args.history_bin).is_file():
        sys.exit(f"no {args.history_bin}: cargo build --release -p tukzmon-core --example demo-history")
    explicit = bool(args.display) or bool(os.environ.get("TUKZMON_DEMO_DISPLAY"))
    args.display = pick_display(args.display or os.environ.get("TUKZMON_DEMO_DISPLAY") or DEFAULT_DISPLAY, explicit)
    names = [p.strip() for p in args.profile.split(",") if p.strip()]
    for name in names:
        if name not in PROFILES:
            sys.exit(f"no profile '{name}'")
    base, made = temp_root()
    (base / ".demo-run").touch()
    log(f"display {args.display}, work in {base}")
    try:
        if args.shot:
            take_shot(args, names[0], base)
            return
        for name in names:
            record(args, name, base)
    finally:
        if made and not args.keep_work:
            safe_rmtree(base)


if __name__ == "__main__":
    main()


##	History:
##		- 2026-11-06: Created, from nemo-anywhere's recorder, with the panel, the
##		  days of history from core/examples/demo-history.rs, and the settings
##		  window's Graphs tab.
