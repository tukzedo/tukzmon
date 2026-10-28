# Nix package of the panel plugin and tukzmon-settings. flake.nix calls it.
#
# Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
# SPDX-License-Identifier: LGPL-2.1-or-later
{
  lib,
  rustPlatform,
  pkg-config,
  qt6,
  glib,
  cairo,
  gtk3,
  xfce4-panel,
  # The build number is this time. Nix's own default is 1980.
  sourceDateEpoch ? null,
}:

rustPlatform.buildRustPackage {
  pname = "tukzmon";
  version = (lib.importTOML ../Cargo.toml).workspace.package.version;

  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../Cargo.toml
      ../Cargo.lock
      ../core
      ../knobs
      ../settings
      ../xfce
    ];
  };
  cargoLock.lockFile = ../Cargo.lock;

  nativeBuildInputs = [
    pkg-config
    qt6.wrapQtAppsHook
  ];
  buildInputs = [
    qt6.qtbase
    qt6.qtwayland
    glib
    cairo
    gtk3
    xfce4-panel
  ];

  cargoBuildFlags = [
    "-p"
    "tukzmon-xfce"
    "-p"
    "tukzmon-settings"
  ];

  env = {
    # xfce/build.rs links these by soname, which Nix's linker wrapper adds
    # no rpath for.
    RUSTFLAGS = "-C link-arg=-Wl,-rpath,${
      lib.makeLibraryPath [
        xfce4-panel
        gtk3
        cairo
        glib
      ]
    }";
  }
  // lib.optionalAttrs (sourceDateEpoch != null) {
    SOURCE_DATE_EPOCH = toString sourceDateEpoch;
  };

  # For each XDG_DATA_DIRS entry <p>/share, the panel reads plugin .desktop
  # files from <p>/share/xfce4/panel/plugins and their libraries from
  # <p>/lib/xfce4/panel/plugins.
  postInstall = ''
    install -Dm644 xfce/tukzmon.desktop -t $out/share/xfce4/panel/plugins
    mkdir -p $out/lib/xfce4/panel/plugins
    mv $out/lib/libtukzmon.so $out/lib/xfce4/panel/plugins/
    # so the plugin runs its own build, not whatever's first on PATH
    ln -s ../../../../bin/tukzmon-settings $out/lib/xfce4/panel/plugins/tukzmon-settings
  '';

  meta = {
    description = "System monitor for the XFCE panel, with a log time axis";
    homepage = "https://github.com/tukzedo/tukzmon";
    license = lib.licenses.lgpl21Plus;
    mainProgram = "tukzmon-settings";
    platforms = [ "x86_64-linux" ];
  };
}
