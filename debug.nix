{ stdenv, makeWrapper, coreutils, gnugrep, gnused, nix }@args:

import ./. (args // { libShellVar = "./src"; })
