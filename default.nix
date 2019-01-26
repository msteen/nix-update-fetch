{ pkgs, stdenv, fetchFromGitHub, rustPlatform, pkgconfig, ncurses, rustup }:

# let src = fetchFromGitHub {
#       owner = "mozilla";
#       repo = "nixpkgs-mozilla";
#       # commit from: 2018-03-27
#       rev = "f61795ea78ea2a489a2cabb27abde254d2a37d25";
#       sha256 = "034m1dryrzh2lmjvk3c0krgip652dql46w5yfwpvh7gavd3iypyw";
#    };
# in
# with import "${src.out}/rust-overlay.nix" pkgs pkgs;

rustPlatform.buildRustPackage rec {
  name = "${pname}-${version}";
  pname = "nix-update-fetch";
  version = "0.1.0";

  src = ./.;

  RUSTC_BOOTSTRAP = 1;

  buildInputs = [ pkgconfig ncurses rustup ];

  cargoSha256 = "05w7a3sp5dynkdqbh56w8xbxpg656r4bcbzgz3j6n648c7gl4rbf";

  meta = with stdenv.lib; {
    description = "Prefetch any fetcher function call, e.g. a package source";
    homepage = https://github.com/msteen/nix-update-fetch;
    license = licenses.mit;
    maintainers = with maintainers; [ msteen ];
    platforms = platforms.all;
  };
}
