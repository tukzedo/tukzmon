#!/usr/bin/env bash
#  shellcheck enable=require-variable-braces  ## Every expansion braced: "${var}", not "$var".

##	Purpose:
##		Installs the newest tukzmon release from GitHub, for this user or for
##		the whole system. It finds the release, checks the package's sha256,
##		says what it's about to do, and asks first. Run again on a current
##		install, it changes nothing.
##		- A user install unpacks the package into the panel's folders under
##		  ~/.local, as utility/dev-install.bash does, with no sudo.
##		- A system install hands the .deb or .rpm to apt-get, dnf, zypper or
##		  yum, with sudo.
##	Syntax:
##		bash <(curl -fsSL https://raw.githubusercontent.com/tukzedo/tukzmon/main/install.bash) [options]
##		--help lists the options.
##	Needs: bash 3.2 or later, curl or wget, and sha256sum, shasum or openssl.
##	  A user install also needs dpkg-deb, rpm2cpio and cpio, bsdtar, or ar.
##	Exit: 0 installed or nothing to do, 1 failed, 2 bad arguments.
##	History: At bottom of script.

##	Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
##	Licensed under The MIT License (MIT). Full text at:
##		https://mit-license.org/
##	SPDX-License-Identifier: MIT


installerVersion="1.0.0"
ownerRepo="tukzedo/tukzmon"
appName="tukzmon"
## {version} {arch} and {ext} are filled in. {ext} is deb or rpm.
assetPattern="tukzmon-{version}-linux-{arch}.{ext}"
sumsPattern="tukzmon-{version}-sha256sums.txt"

apiBase="https://api.github.com/repos/${ownerRepo}"
dlBase="https://github.com/${ownerRepo}/releases/download"
rawBase="https://raw.githubusercontent.com/${ownerRepo}/main"
diyUrl="https://github.com/${ownerRepo}#diy"


fFail() {
	local first="${1}"; shift
	echo "Error: ${first}" >&2
	while (($#)); do echo "  ${1}" >&2; shift; done
	echo >&2
	exit 1
}
fBadArg() { echo "Error: ${1}" >&2; echo "  Run with --help to see the options." >&2; echo >&2; exit 2; }

fHelp() {
	cat << EOF
${appName} installer ${installerVersion}

Installs the newest ${appName} release from GitHub, after checking its sha256.
It says what it's about to do and asks first. Run again on a current install,
it changes nothing.

Usage:
  bash <(curl -fsSL ${rawBase}/install.bash) [options]

Options:
  --release stable|dev   stable (default): the newest full release, or the
                                           newest prerelease while there's none
                         dev:              the newest release, prereleases too
  --target  user|system  user (default):   the panel's folders under ~/.local,
                                           with no sudo
                         system:           the .deb or .rpm, through apt-get,
                                           dnf, zypper or yum, with sudo
  --yes, -y              don't ask before installing
  --version              print this installer's version
  --help, -h             this text

The OS, the CPU and which package to use are worked out here.

EOF
}

fLower() { echo "${1}" | tr '[:upper:]' '[:lower:]'; }

## Https on the first request and every redirect, or a redirect could walk
## the download down to http, where the checksum stops meaning anything.
httpsOnly_curl=(--proto '=https' --proto-redir '=https')
httpsOnly_wget=(--https-only)

fGet() {   ## fGet URL OUTFILE
	if [[ "${dlTool}" == curl ]]; then curl -fsSL "${httpsOnly_curl[@]}" -o "${2}" "${1}"
	else wget -qO "${2}" "${httpsOnly_wget[@]}" "${1}"; fi
}

## Only the API is rate limited per IP, so only it gets the optional token,
## through a 0600 file rather than argv, where ps shows it to everyone.
authFile=""
fAuthInit() {
	[[ -n "${GITHUB_TOKEN:-}" ]] || return 0
	authFile="${tmpDir}/auth"
	## Only curl's config format takes quotes.
	if [[ "${dlTool}" == curl ]]; then
		( umask 077; printf 'header = "Authorization: Bearer %s"\n' "${GITHUB_TOKEN}" > "${authFile}" )
	else
		( umask 077; printf 'header = Authorization: Bearer %s\n' "${GITHUB_TOKEN}" > "${authFile}" )
	fi
}

## The body is kept whatever the status, since GitHub says why in it.
fApi() {   ## fApi URL OUTFILE
	local url="${1}" out="${2}" code="" rc=0
	if [[ "${dlTool}" == curl ]]; then
		if [[ -n "${authFile}" ]]; then
			code="$(curl -sSL "${httpsOnly_curl[@]}" --config "${authFile}" -o "${out}" -w '%{http_code}' "${url}" 2> "${out}.err")" || rc=$?
		else
			code="$(curl -sSL "${httpsOnly_curl[@]}" -o "${out}" -w '%{http_code}' "${url}" 2> "${out}.err")" || rc=$?
		fi
		((rc == 0)) || return 1
		case "${code}" in 2??) return 0 ;; esac
		echo "HTTP ${code}" >> "${out}.err"
		return 1
	fi
	if [[ -n "${authFile}" ]]; then
		WGETRC="${authFile}" wget -nv --content-on-error -O "${out}" "${httpsOnly_wget[@]}" "${url}" 2> "${out}.err"
	else
		wget -nv --content-on-error -O "${out}" "${httpsOnly_wget[@]}" "${url}" 2> "${out}.err"
	fi
}

fSha256() {
	case "${shaTool}" in
		sha256sum) sha256sum "${1}" | awk '{print $1}' ;;
		shasum)    shasum -a 256 "${1}" | awk '{print $1}' ;;
		openssl)   openssl dgst -sha256 "${1}" | awk '{print $NF}' ;;
	esac
}

## One line per release: tag, draft or -, pre or full. No jq: JSON strings
## hold no raw newline, so the body goes on one line and each "tag_name"
## starts a new one. A release's draft and prerelease flags come after its
## tag and before the next one's.
fReleaseRows() {
	tr -d '\r\n' | sed 's/"tag_name"/\
"tag_name"/g' | awk '/^"tag_name"/ {
		tag = $0
		sub(/^"tag_name"[ \t]*:[ \t]*"/, "", tag); sub(/".*/, "", tag)
		draft = ($0 ~ /"draft"[ \t]*:[ \t]*true/) ? "draft" : "-"
		pre = ($0 ~ /"prerelease"[ \t]*:[ \t]*true/) ? "pre" : "full"
		if (tag != "") print tag, draft, pre
	}'
}

## -1, 0 or 1 for one dotted field. Numbers sort below words, and beta10 is
## above beta3.
fFieldCmp() {
	local a="${1}" b="${2}" num='^[0-9]+$' word='^([^0-9]*)([0-9]+)$'
	local aStem="" aNum="" bStem="" bNum=""
	if [[ ${a} =~ ${num} && ${b} =~ ${num} ]]; then
		if ((10#${a} > 10#${b})); then echo 1; elif ((10#${a} < 10#${b})); then echo -1; else echo 0; fi
		return 0
	fi
	if [[ ${a} =~ ${num} ]]; then echo -1; return 0; fi
	if [[ ${b} =~ ${num} ]]; then echo 1; return 0; fi
	if [[ ${a} =~ ${word} ]]; then aStem="${BASH_REMATCH[1]}"; aNum="${BASH_REMATCH[2]}"; fi
	if [[ ${b} =~ ${word} ]]; then bStem="${BASH_REMATCH[1]}"; bNum="${BASH_REMATCH[2]}"; fi
	if [[ -n "${aNum}" && -n "${bNum}" && "${aStem}" == "${bStem}" ]]; then
		fFieldCmp "${aNum}" "${bNum}"; return 0
	fi
	if [[ ${a} < ${b} ]]; then echo -1; elif [[ ${a} > ${b} ]]; then echo 1; else echo 0; fi
}

## Dotted lists, field by field. MISSING is what a shorter list's missing
## field counts as: -1 in a prerelease, or 0 in the core, where 1.0 is 1.0.0.
fListCmp() {   ## fListCmp A B MISSING
	local a="${1}" b="${2}" missing="${3}" c=""
	while [[ -n "${a}" || -n "${b}" ]]; do
		if [[ -z "${a}" ]]; then if [[ "${missing}" == 0 ]]; then a=0; else echo -1; return 0; fi; fi
		if [[ -z "${b}" ]]; then if [[ "${missing}" == 0 ]]; then b=0; else echo 1; return 0; fi; fi
		c="$(fFieldCmp "${a%%.*}" "${b%%.*}")"
		[[ "${c}" == 0 ]] || { echo "${c}"; return 0; }
		case "${a}" in *.*) a="${a#*.}" ;; *) a="" ;; esac
		case "${b}" in *.*) b="${b#*.}" ;; *) b="" ;; esac
	done
	echo 0
}

## True when tag A is above B in semver order, where 1.0.0-beta.2 is below
## 1.0.0. sort -V puts it above.
fNewer() {   ## fNewer A B
	local a="${1#v}" b="${2#v}" aPre="" bPre="" c=""
	a="${a%%+*}"; b="${b%%+*}"
	case "${a}" in *-*) aPre="${a#*-}"; a="${a%%-*}" ;; esac
	case "${b}" in *-*) bPre="${b#*-}"; b="${b%%-*}" ;; esac
	c="$(fListCmp "${a}" "${b}" 0)"
	[[ "${c}" == 0 ]] || { [[ "${c}" == 1 ]]; return; }
	[[ -n "${aPre}" ]] || { [[ -n "${bPre}" ]]; return; }
	[[ -n "${bPre}" ]] || return 1
	[[ "$(fListCmp "${aPre}" "${bPre}" -1)" == 1 ]]
}

## The highest tag in the rows on stdin, with no drafts, and for stable no
## prereleases. Prints nothing when there's none.
fPickTag() {   ## fPickTag stable|dev
	local want="${1}" tag draft pre best=""
	while read -r tag draft pre; do
		[[ "${draft}" == - ]] || continue
		[[ "${want}" == dev || "${pre}" == full ]] || continue
		if [[ -z "${best}" ]] || fNewer "${tag}" "${best}"; then best="${tag}"; fi
	done
	[[ -z "${best}" ]] || echo "${best}"
}

## The temp folder is the one mktemp made, so that's the only one removed.
tmpDir=""
fCleanup() {
	if [[ -n "${tmpDir}" && "${tmpDir}" == /*/tukzmon-install.?????? && "${tmpDir}" != *..* && -d "${tmpDir}" && ! -L "${tmpDir}" ]]; then
		## busybox's rm has no --one-file-system.
		rm -rf --one-file-system -- "${tmpDir}" 2> /dev/null || rm -rf -- "${tmpDir}"
	fi
}

## 0 yes, 1 no, 2 nobody to ask. The terminal first, since with curl | bash
## the script itself is on stdin.
fConfirm() {
	local answer=""
	printf '%s [y/N]: ' "${1}"
	if { : < /dev/tty; } 2> /dev/null; then
		read -r answer < /dev/tty || { echo; return 2; }
	else
		read -r answer || { echo; return 2; }
	fi
	case "${answer}" in y|Y|yes|Yes|YES) return 0 ;; *) return 1 ;; esac
}

## The package's files into DIR, by whichever tool is here.
fUnpack() {   ## fUnpack PACKAGE DIR
	local pkg="${1}" dir="${2}" member=""
	case "${unpackTool}" in
		dpkg-deb) dpkg-deb -x "${pkg}" "${dir}" ;;
		rpm2cpio) ( cd "${dir}" && rpm2cpio "${pkg}" | cpio -idm 2> /dev/null ) ;;
		bsdtar)   bsdtar -xOf "${pkg}" 'data.tar*' | bsdtar -xf - -C "${dir}" ;;
		ar)
			member="$(ar t "${pkg}" | grep '^data\.tar' | head -n 1)"
			[[ -n "${member}" ]] || return 1
			ar p "${pkg}" "${member}" > "${dir}/${member}" && tar -xf "${dir}/${member}" -C "${dir}" ;;
	esac
}

fShown() { if [[ "${1}" == "${HOME}"/* ]]; then echo "~${1#"${HOME}"}"; else echo "${1}"; fi; }

## Copy then rename, so a running panel keeps the old file it has open.
fPut() {   ## fPut FILE DIR MODE
	local name="${1##*/}"
	cp "${1}" "${2}/.${name}.new" || fFail "couldn't write ${name} to ${2}"
	chmod "${3}" "${2}/.${name}.new"
	mv -f "${2}/.${name}.new" "${2}/${name}" || fFail "couldn't put ${name} in place in ${2}"
}

fMain() {
	local release="stable" target="user" assumeYes=0
	echo
	while (($#)); do case "${1}" in
		--release)   (($# >= 2)) || fBadArg "--release needs a value: stable or dev"; release="${2}"; shift 2 ;;
		--release=*) release="${1#*=}"; shift ;;
		--target)    (($# >= 2)) || fBadArg "--target needs a value: user or system"; target="${2}"; shift 2 ;;
		--target=*)  target="${1#*=}"; shift ;;
		-y|--yes)    assumeYes=1; shift ;;
		--version)   echo "${appName} installer ${installerVersion}"; echo; exit 0 ;;
		-h|--help)   fHelp; exit 0 ;;
		*)           fBadArg "unknown option: ${1}" ;;
	esac; done
	case "${release}" in stable|dev) ;; *) fBadArg "--release is stable or dev, not '${release}'" ;; esac
	case "${target}" in user|system) ;; *) fBadArg "--target is user or system, not '${target}'" ;; esac

	local osName archName arch=""
	osName="$(uname -s 2> /dev/null || echo unknown)"
	[[ "$(fLower "${osName}")" == linux ]] || fFail "there's no ${appName} build for ${osName} yet, only for Linux"
	archName="$(uname -m 2> /dev/null || echo unknown)"
	case "$(fLower "${archName}")" in
		x86_64|amd64)  arch="x86_64" ;;
		aarch64|arm64) arch="arm64" ;;
		*)             fFail "there's no ${appName} build for ${archName}" "To build it yourself: ${diyUrl}" ;;
	esac

	dlTool=""; shaTool=""
	if command -v curl > /dev/null 2>&1; then dlTool=curl
	elif command -v wget > /dev/null 2>&1; then dlTool=wget
	else fFail "neither curl nor wget is installed" "Install one and run this again."; fi
	if command -v sha256sum > /dev/null 2>&1; then shaTool=sha256sum
	elif command -v shasum > /dev/null 2>&1; then shaTool=shasum
	elif command -v openssl > /dev/null 2>&1; then shaTool=openssl
	else fFail "there's no sha256sum, shasum or openssl here" "Nothing is installed that can't be checked."; fi

	## What reads the package: unpacking for a user, a package manager for
	## the system.
	local ext="" pkgTool="" sudoCmd=""
	unpackTool=""
	if [[ "${target}" == user ]]; then
		if command -v dpkg-deb > /dev/null 2>&1; then unpackTool=dpkg-deb; ext=deb
		elif command -v rpm2cpio > /dev/null 2>&1 && command -v cpio > /dev/null 2>&1; then unpackTool=rpm2cpio; ext=rpm
		elif command -v bsdtar > /dev/null 2>&1; then unpackTool=bsdtar; ext=deb
		elif command -v ar > /dev/null 2>&1 && command -v tar > /dev/null 2>&1; then unpackTool="ar"; ext=deb
		else fFail "there's nothing here that unpacks a .deb or .rpm" "Install dpkg, rpm2cpio and cpio, libarchive's bsdtar, or binutils, and run this again."; fi
	else
		if command -v apt-get > /dev/null 2>&1 && command -v dpkg-deb > /dev/null 2>&1; then pkgTool=apt-get; ext=deb
		elif command -v rpm > /dev/null 2>&1; then
			if command -v dnf > /dev/null 2>&1; then pkgTool=dnf
			elif command -v zypper > /dev/null 2>&1; then pkgTool=zypper
			elif command -v yum > /dev/null 2>&1; then pkgTool=yum; fi
			ext=rpm
		fi
		[[ -n "${pkgTool}" ]] || fFail "there's no apt-get, dnf, zypper or yum here for the .deb or .rpm" "Use --target user to install for this user instead."
		if [[ "$(id -u)" != 0 ]]; then
			command -v sudo > /dev/null 2>&1 || fFail "a system install needs root, and there's no sudo" "Run it as root, or use --target user."
			sudoCmd=sudo
		fi
	fi

	trap fCleanup EXIT
	trap 'exit 130' HUP INT TERM
	tmpDir="$(mktemp -d "${TMPDIR:-/tmp}/tukzmon-install.XXXXXX" 2> /dev/null)" || { tmpDir=""; fFail "couldn't make a temp folder"; }
	fAuthInit

	## A failed call is an error, never "no release".
	echo "Looking for the newest ${release} release of ${appName} ..."
	local tag="" note="" listFile="${tmpDir}/releases.json"
	if ! fApi "${apiBase}/releases?per_page=100" "${listFile}"; then
		if grep -qi 'rate limit' "${listFile}" 2> /dev/null; then
			fFail "GitHub's API rate limit is used up for this address" "Wait an hour, or set GITHUB_TOKEN to a personal access token and run this again."
		fi
		fFail "couldn't read the release list from github.com/${ownerRepo}" \
			"Check the network or HTTPS_PROXY." "Detail: $(tr '\n' ' ' < "${listFile}.err" 2> /dev/null)"
	fi
	tag="$(fReleaseRows < "${listFile}" | fPickTag "${release}")"
	if [[ -z "${tag}" && "${release}" == stable ]]; then
		tag="$(fReleaseRows < "${listFile}" | fPickTag dev)"
		[[ -z "${tag}" ]] || note=", the newest prerelease, since there's no stable release yet"
	fi
	[[ -n "${tag}" ]] || fFail "github.com/${ownerRepo} has no release yet" "To build it yourself: ${diyUrl}"
	local version="${tag#v}" asset sums wantSha haveSha
	asset="${assetPattern//\{version\}/${version}}"; asset="${asset//\{arch\}/${arch}}"; asset="${asset//\{ext\}/${ext}}"
	sums="${sumsPattern//\{version\}/${version}}"

	fGet "${dlBase}/${tag}/${sums}" "${tmpDir}/${sums}" 2> /dev/null \
		|| fFail "release ${tag} has no checksums file (${sums})" "Nothing is installed that can't be checked." \
			"Release page: https://github.com/${ownerRepo}/releases/tag/${tag}"
	wantSha="$(awk -v want="${asset}" '{ name = $2; sub(/^\*/, "", name); if (name == want) { print $1; exit } }' "${tmpDir}/${sums}")"
	if [[ -z "${wantSha}" ]]; then
		{
			echo "Error: release ${tag} has no ${asset}."
			echo "  It has:"
			awk '{ name = $2; sub(/^\*/, "", name); print "    " name }' "${tmpDir}/${sums}"
			echo "  To build it yourself: ${diyUrl}"
			echo
		} >&2
		exit 1
	fi

	## Into the temp folder before the plan, so the plan can say what would
	## change. Nothing outside it is touched until the answer is yes.
	local pkg="${tmpDir}/${asset}"
	echo "Downloading ${asset} ..."
	fGet "${dlBase}/${tag}/${asset}" "${pkg}" || fFail "the download failed" "URL: ${dlBase}/${tag}/${asset}"
	haveSha="$(fSha256 "${pkg}")"
	[[ "${haveSha}" == "${wantSha}" ]] || fFail "the checksum doesn't match, so nothing was installed" \
		"expected ${wantSha}" "got      ${haveSha}" "The download was damaged or tampered with. Try again, and report it if it happens again."
	echo "Checksum OK."

	local -a todo=() notes=()
	local i=0 panelUp=0 shareDir="" libDir="" have="" want=""
	if pgrep -x -u "$(id -u)" xfce4-panel > /dev/null 2>&1; then panelUp=1; fi
	if [[ "${target}" == user ]]; then
		local dataHome="${XDG_DATA_HOME:-${HOME}/.local/share}" panelLib="" libTail="" unpacked="${tmpDir}/unpacked" lib="" missing=""
		[[ "${dataHome}" == */share ]] || fFail "the panel reads no user plugins when XDG_DATA_HOME ('${dataHome}') doesn't end in /share"
		## The panel pairs its user data folder with a lib folder under the
		## same prefix, with its own lib folder's tail, such as
		## lib/x86_64-linux-gnu on Debian. ldconfig is in /usr/sbin, which a
		## Debian user's PATH doesn't have.
		panelLib="$(PATH="${PATH}:/usr/sbin:/sbin" ldconfig -p 2> /dev/null | grep -m 1 -F 'libxfce4panel-2.0.so.4 ' | sed 's/.*=> //' || true)"
		if [[ -n "${panelLib}" ]]; then
			libTail="${panelLib%/*}"; libTail="lib${libTail##*/lib}"
		else
			libTail="lib/$(uname -m)-linux-gnu"
			notes+=("The XFCE panel's library, libxfce4panel-2.0.so.4, isn't installed. tukzmon needs the XFCE 4.20 panel.")
		fi
		shareDir="${dataHome}/xfce4/panel/plugins"
		libDir="${dataHome%/share}/${libTail}/xfce4/panel/plugins"

		mkdir "${unpacked}" || fFail "couldn't make ${unpacked}"
		fUnpack "${pkg}" "${unpacked}" || fFail "couldn't unpack ${asset}"
		lib="$(find "${unpacked}" -name libtukzmon.so -type f | head -n 1)"
		[[ -n "${lib}" && -f "${unpacked}/usr/bin/tukzmon-settings" && -f "${unpacked}/usr/share/xfce4/panel/plugins/tukzmon.desktop" ]] \
			|| fFail "${asset} doesn't hold the files it should"
		## Source, folder, mode, three to a row.
		set -- "${lib}" "${libDir}" 0644 "${unpacked}/usr/bin/tukzmon-settings" "${libDir}" 0755 \
			"${unpacked}/usr/share/xfce4/panel/plugins/tukzmon.desktop" "${shareDir}" 0644
		while (($#)); do
			have="${2}/${1##*/}"
			if [[ ! -f "${have}" || "$(fSha256 "${have}")" != "$(fSha256 "${1}")" ]]; then todo+=("${1}" "${2}" "${3}"); fi
			shift 3
		done
		missing="$(ldd "${lib}" "${unpacked}/usr/bin/tukzmon-settings" 2> /dev/null | awk '/not found/ { print $1 }' | sort -u | tr '\n' ' ' || true)"
		[[ -z "${missing}" ]] || notes+=("These libraries aren't installed: ${missing}tukzmon needs the XFCE 4.20 panel and Qt 6.")
		if command -v dpkg-query > /dev/null 2>&1 && [[ "$(dpkg-query -W -f '${db:Status-Status}' tukzmon 2> /dev/null || true)" == installed ]] \
			|| { command -v rpm > /dev/null 2>&1 && rpm -q tukzmon > /dev/null 2>&1; }; then
			notes+=("The tukzmon package is installed too. The panel loads this user copy first.")
		fi
	else
		local pkgVer="" haveVer=""
		if [[ "${ext}" == deb ]]; then
			pkgVer="$(dpkg-deb -f "${pkg}" Version 2> /dev/null || true)"
			have="$(dpkg-query -W -f '${db:Status-Status} ${Version}' tukzmon 2> /dev/null || true)"
			[[ "${have}" != "installed "* ]] || haveVer="${have#installed }"
		else
			pkgVer="$(rpm -qp --qf '%{VERSION}-%{RELEASE}' "${pkg}" 2> /dev/null || true)"
			if rpm -q tukzmon > /dev/null 2>&1; then haveVer="$(rpm -q --qf '%{VERSION}-%{RELEASE}' tukzmon 2> /dev/null || true)"; fi
		fi
		[[ -n "${pkgVer}" ]] || fFail "couldn't read the version in ${asset}"
		[[ "${haveVer}" == "${pkgVer}" ]] || todo+=("${pkg}")
		## A user install goes ahead of the package, for that user.
		for want in "${XDG_DATA_HOME:-${HOME}/.local/share}/xfce4/panel/plugins/tukzmon.desktop" "${HOME}"/.local/lib*/xfce4/panel/plugins/libtukzmon.so "${HOME}"/.local/lib/*/xfce4/panel/plugins/libtukzmon.so; do
			[[ ! -e "${want}" ]] || notes+=("$(fShown "${want}") is a user install, which the panel loads ahead of the package. Remove it to use the package.")
		done
	fi

	if ((${#todo[@]} == 0)); then
		if [[ "${target}" == user ]]; then echo "Already up to date: $(fShown "${libDir}") has ${appName} ${tag}. Nothing to do."
		else echo "Already up to date: the ${appName} package is ${pkgVer}. Nothing to do."; fi
		for ((i = 0; i < ${#notes[@]}; i++)); do echo "Note: ${notes[i]}"; done
		echo
		exit 0
	fi

	echo
	echo "Plan:"
	echo "  Release:   ${appName} ${tag}${note}"
	echo "  Platform:  linux-${arch}"
	echo "  Package:   ${asset}, sha256 checked"
	if [[ "${target}" == user ]]; then
		for ((i = 0; i < ${#todo[@]}; i += 3)); do
			if ((i == 0)); then printf '  Install:   %s\n' "$(fShown "${todo[i + 1]}/${todo[i]##*/}")"; else printf '             %s\n' "$(fShown "${todo[i + 1]}/${todo[i]##*/}")"; fi
		done
	else
		echo "  Install:   ${sudoCmd:+sudo }${pkgTool} install${haveVer:+, over ${haveVer}}"
	fi
	((! panelUp)) || echo "  Then:      xfce4-panel --restart, so it loads the new build"
	for ((i = 0; i < ${#notes[@]}; i++)); do echo "  Note:      ${notes[i]}"; done
	echo
	if ((! assumeYes)); then
		local answered=0
		fConfirm "Go ahead?" || answered=$?
		((answered != 2)) || fFail "there's no terminal to ask on" "Run it with --yes to install without asking."
		((answered == 0)) || { echo "Stopped. Nothing was touched."; echo; exit 0; }
		echo
	fi

	if [[ "${target}" == user ]]; then
		mkdir -p "${libDir}" "${shareDir}" || fFail "couldn't make ${libDir} and ${shareDir}"
		for ((i = 0; i < ${#todo[@]}; i += 3)); do fPut "${todo[i]}" "${todo[i + 1]}" "${todo[i + 2]}"; done
		echo "Installed ${appName} ${tag} to $(fShown "${libDir}") and $(fShown "${shareDir}")"
	else
		local yesFlag=""
		((! assumeYes)) || yesFlag="-y"
		## apt's sandbox user has to read the package.
		chmod 0755 "${tmpDir}"; chmod 0644 "${pkg}"
		case "${pkgTool}" in
			zypper) ${sudoCmd:+"${sudoCmd}"} zypper install ${yesFlag:+"${yesFlag}"} --allow-unsigned-rpm "${pkg}" ;;
			*)      ${sudoCmd:+"${sudoCmd}"} "${pkgTool}" install ${yesFlag:+"${yesFlag}"} "${pkg}" ;;
		esac || fFail "${pkgTool} didn't install ${asset}"
		echo "Installed the ${appName} ${tag} package"
	fi

	if ((panelUp)); then xfce4-panel --restart || echo "Note: the panel didn't restart. Run 'xfce4-panel --restart' to load the new build."; fi
	if command -v xfconf-query > /dev/null 2>&1 && ! grep -q 'tukzmon$' <<< "$(xfconf-query -c xfce4-panel -lv 2> /dev/null || true)"; then
		echo "To add it to a panel: right-click the panel, then Panel, Add New Items, tukzmon."
	fi
	echo
}


set -Eeuo pipefail
shopt -s inherit_errexit 2> /dev/null || true
## Sourced, it only defines the functions.
if [[ "${BASH_SOURCE[0]:-}" == "${0}" || -z "${BASH_SOURCE[0]:-}" ]]; then
	## "$@" with nothing in it is unbound under set -u in bash 3.2.
	if (($#)); then fMain "${@}"; else fMain; fi
fi


##	History:
##		- 2026-11-10: Created, from silkterm's install.bash. User installs
##		  unpack the package into the folders dev-install.bash uses, and
##		  system installs hand it to the package manager.
