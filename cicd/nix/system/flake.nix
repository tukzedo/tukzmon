# A NixOS system with tukzmon's module, for cicd/nix/check.bash, which runs it
# in a container with the repo's files at /src. It follows the repo's nixpkgs.
#
# Copyright © 2026 t00mietum (CryptogID: ปʬϝღถɔ4რఠΔթะ9ƾǝu)
# SPDX-License-Identifier: MIT
{
  inputs.tukzmon.url = "path:/src";
  inputs.nixpkgs.follows = "tukzmon/nixpkgs";

  outputs =
    { nixpkgs, tukzmon, ... }:
    let
      sys =
        enableXfce:
        nixpkgs.lib.nixosSystem {
          system = "x86_64-linux";
          modules = [
            tukzmon.nixosModules.default
            {
              programs.tukzmon.enable = true;
              services.xserver.desktopManager.xfce.enable = enableXfce;
              boot.loader.grub.enable = false;
              fileSystems."/" = {
                device = "/dev/null";
                fsType = "ext4";
              };
              system.stateVersion = "26.05";
            }
          ];
        };
      # "package links" when the system has the package and both links.
      show =
        c:
        let
          links = builtins.all (p: builtins.elem p c.config.environment.pathsToLink) [
            "/share/xfce4"
            "/lib/xfce4"
          ];
          hasPkg = builtins.any (p: (p.pname or "") == "tukzmon") c.config.environment.systemPackages;
        in
        builtins.concatStringsSep " " (
          (if hasPkg then [ "package" ] else [ ]) ++ (if links then [ "links" ] else [ ])
        );
    in
    {
      plain = show (sys false);
      xfce = show (sys true);
      path = (sys false).config.system.path;
    };
}
