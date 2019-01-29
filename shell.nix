{ nixpkgsPath ? <nixpkgs> }:

let
  mozOverlay = import (builtins.fetchTarball https://github.com/mozilla/nixpkgs-mozilla/archive/master.tar.gz);
  pkgs = import nixpkgsPath { overlays = [ mozOverlay ]; };
in

# This package requires `edition = "2018"` which is only supported since version 1.31 of the Rust compiler.
# https://blog.rust-lang.org/2018/12/06/Rust-1.31-and-rust-2018.html
if pkgs.lib.versionOlder pkgs.rustc.version "1.31" then throw (pkgs.lib.removeSuffix "\n" ''
  Rust compiler version >= 1.31 is required for this package. Try it with a more recent Nixpkgs instead (> 18.09):
    nix-shell --arg nixpkgsPath '<nixpkgs-unstable>'
    nix-shell -I nixpkgs=channel:nixos-unstable
'') else

pkgs.stdenv.mkDerivation {
  name = "moz-overlay-rust";
  buildInputs = [
    pkgs.latest.rustChannels.stable.rust
  ];
}
