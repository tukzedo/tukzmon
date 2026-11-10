// SPDX-License-Identifier: LGPL-2.1-or-later
// Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)

//! Whether this desktop session is a remote one, such as an RDP or NoMachine
//! virtual desktop, so the graphs can draw less often. See design.md Drawing.
//!
//! It goes by what the session was started with, so it doesn't see a viewer
//! that joins a local desktop later.

/// Variables a remote desktop server sets in the sessions it starts: xrdp,
/// X2Go, Chrome Remote Desktop, and the classic vncserver script.
const REMOTE_VARS: [&str; 4] = [
	"XRDP_SESSION",
	"X2GO_SESSION",
	"CHROME_REMOTE_DESKTOP_SESSION",
	"VNCDESKTOP",
];

/// The PAM services logind names for sessions a remote desktop server opens:
/// NoMachine's virtual desktops and xrdp.
const REMOTE_SERVICES: [&str; 2] = ["nx", "xrdp-sesman"];

/// Where the desktop this runs on is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Session {
	/// On this box's own screen, or not known to be anywhere else.
	#[default]
	Local,
	/// In a remote desktop session, where each frame goes over the network.
	Remote,
}

/// The session this process runs in.
pub fn session() -> Session {
	if detected() {
		Session::Remote
	} else {
		Session::Local
	}
}

/// True when this process runs in a remote desktop session.
pub fn detected() -> bool {
	let var = |name: &str| std::env::var_os(name).is_some_and(|v| !v.is_empty());
	// logind's own record of the session, as sd_session_is_remote reads it
	let session = std::env::var("XDG_SESSION_ID")
		.ok()
		.filter(|id| !id.is_empty() && !id.contains('/'))
		.and_then(|id| std::fs::read_to_string(format!("/run/systemd/sessions/{id}")).ok());
	detected_by(var, session.as_deref())
}

/// As [`detected`], from whether each variable is set, and the text of the
/// session's logind file, if there is one.
pub fn detected_by(var: impl Fn(&str) -> bool, session: Option<&str>) -> bool {
	REMOTE_VARS.iter().any(|v| var(v))
		|| session.is_some_and(|s| {
			s.lines().any(|line| match line.split_once('=') {
				Some(("REMOTE", v)) => v == "1",
				Some(("SERVICE", v)) => REMOTE_SERVICES.contains(&v),
				_ => false,
			})
		})
}

#[cfg(test)]
mod tests {
	use super::*;

	// Test ID: df3nf
	#[test]
	fn a_remote_session_is_told_by_its_variables_or_by_logind() {
		let none = |_: &str| false;
		let local = "UID=1001\nREMOTE=0\nTYPE=x11\nDISPLAY=:0\nSERVICE=lightdm\n";
		assert!(!detected_by(none, None));
		assert!(!detected_by(none, Some(local)));
		for v in REMOTE_VARS {
			assert!(detected_by(|name| name == v, Some(local)), "{v}");
		}
		assert!(!detected_by(|name| name == "DISPLAY", None));
		// NoMachine's virtual desktops say REMOTE=0
		assert!(detected_by(none, Some("REMOTE=0\nTYPE=x11\nSERVICE=nx\n")));
		assert!(detected_by(none, Some("REMOTE=1\nSERVICE=sshd\n")));
		assert!(detected_by(none, Some("SERVICE=xrdp-sesman\n")));
		assert!(!detected_by(none, Some("SERVICE=nxx\nREMOTE_HOST=1\n")));
	}
}
